// Copyright (c) 2013-2016 Sandstorm Development Group, Inc. and contributors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use futures::stream::FuturesUnordered;
use futures::unsync::mpsc;
use futures::{Async, Future, Stream};

use std::cell::RefCell;
use std::rc::Rc;

enum EnqueuedTask<T, E> {
    Task(Box<Future<Item = T, Error = E>>),
    Terminate(Result<(), E>),
}

enum TaskInProgress<E> {
    Task(Box<Future<Item = (), Error = ()>>),
    Terminate(Option<Result<(), E>>),
}

enum TaskDone<E> {
    Continue,
    Terminate(Result<(), E>),
}

impl<E> Future for TaskInProgress<E> {
    type Item = TaskDone<E>;
    type Error = ();

    fn poll(&mut self) -> ::futures::Poll<Self::Item, Self::Error> {
        match *self {
            TaskInProgress::Terminate(ref mut r) => Ok(::futures::Async::Ready(
                TaskDone::Terminate(r.take().unwrap()),
            )),
            TaskInProgress::Task(ref mut f) => match f.poll() {
                Ok(::futures::Async::Ready(())) => Ok(::futures::Async::Ready(TaskDone::Continue)),
                Ok(::futures::Async::NotReady) => Ok(::futures::Async::NotReady),
                Err(_e) => unreachable!(),
            },
        }
    }
}

#[must_use = "a TaskSet does nothing unless polled"]
pub struct TaskSet<T, E> {
    enqueued: Option<mpsc::UnboundedReceiver<EnqueuedTask<T, E>>>,
    in_progress: FuturesUnordered<TaskInProgress<E>>,
    reaper: Rc<RefCell<Box<TaskReaper<T, E>>>>,
}

impl<T, E> TaskSet<T, E>
where
    T: 'static,
    E: 'static,
{
    pub fn new(reaper: Box<TaskReaper<T, E>>) -> (TaskSetHandle<T, E>, TaskSet<T, E>)
    where
        E: 'static,
        T: 'static,
        E: ::std::fmt::Debug,
    {
        let (sender, receiver) = mpsc::unbounded();

        let mut set = TaskSet {
            enqueued: Some(receiver),
            in_progress: FuturesUnordered::new(),
            reaper: Rc::new(RefCell::new(reaper)),
        };

        // If the FuturesUnordered ever gets empty, its stream will terminate, which
        // is not what we want. So we make sure there is always at least one future in it.
        set.in_progress
            .push(TaskInProgress::Task(Box::new(::futures::future::empty())));

        let handle = TaskSetHandle { sender: sender };

        (handle, set)
    }
}

#[derive(Clone)]
pub struct TaskSetHandle<T, E> {
    sender: mpsc::UnboundedSender<EnqueuedTask<T, E>>,
}

impl<T, E> TaskSetHandle<T, E>
where
    T: 'static,
    E: 'static,
{
    pub fn add<F>(&mut self, f: F)
    where
        F: Future<Item = T, Error = E> + 'static,
    {
        let _ = self.sender.unbounded_send(EnqueuedTask::Task(Box::new(f)));
    }

    pub fn terminate(&mut self, result: Result<(), E>) {
        let _ = self.sender.unbounded_send(EnqueuedTask::Terminate(result));
    }
}

pub trait TaskReaper<T, E>
where
    T: 'static,
    E: 'static,
{
    fn task_succeeded(&mut self, _value: T) {}
    fn task_failed(&mut self, error: E);
}

impl<T, E> Future for TaskSet<T, E>
where
    T: 'static,
    E: 'static,
{
    type Item = ();
    type Error = E;

    fn poll(&mut self) -> ::futures::Poll<Self::Item, Self::Error> {
        let mut enqueued_stream_complete = false;
        if let Some(ref mut enqueued) = self.enqueued {
            loop {
                match enqueued.poll() {
                    Err(_) => unreachable!(),
                    Ok(Async::NotReady) => break,
                    Ok(Async::Ready(None)) => {
                        enqueued_stream_complete = true;
                        break;
                    }
                    Ok(Async::Ready(Some(EnqueuedTask::Terminate(r)))) => {
                        self.in_progress.push(TaskInProgress::Terminate(Some(r)));
                    }
                    Ok(Async::Ready(Some(EnqueuedTask::Task(f)))) => {
                        let reaper = Rc::downgrade(&self.reaper);
                        self.in_progress
                            .push(TaskInProgress::Task(Box::new(f.then(move |r| {
                                match reaper.upgrade() {
                                    None => Ok(()), // TaskSet must have been dropped.
                                    Some(rc_reaper) => {
                                        match r {
                                            Ok(v) => rc_reaper.borrow_mut().task_succeeded(v),
                                            Err(e) => rc_reaper.borrow_mut().task_failed(e),
                                        }
                                        Ok(())
                                    }
                                }
                            }))));
                    }
                }
            }
        }
        if enqueued_stream_complete {
            drop(self.enqueued.take());
        }

        loop {
            match self.in_progress.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(_) => unreachable!(),
                Ok(Async::Ready(v)) => match v {
                    None => return Ok(Async::Ready(())),
                    Some(TaskDone::Continue) => (),
                    Some(TaskDone::Terminate(Ok(()))) => return Ok(Async::Ready(())),
                    Some(TaskDone::Terminate(Err(e))) => return Err(e),
                },
            }
        }
    }
}
