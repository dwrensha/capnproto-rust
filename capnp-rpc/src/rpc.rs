// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
// Licensed under the MIT License:
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

use std::pin::Pin;
use std::task::{Context, Poll};

use capnp::any_pointer;
use capnp::capability::Promise;
use capnp::private::capability::{
    ClientHook, ParamsHook, PipelineHook, PipelineOp, RequestHook, ResponseHook, ResultsHook,
};
use capnp::Error;

use futures_channel::oneshot;
use futures_core::Future;
use futures_util::{future, FutureExt, TryFutureExt};

use std::cell::{Cell, RefCell};
use std::collections::binary_heap::BinaryHeap;
use std::collections::hash_map::HashMap;
use std::rc::{Rc, Weak};
use std::vec::Vec;
use std::{cmp, mem};

use crate::attach::Attach;
use crate::local::ResultsDoneHook;
use crate::rpc_capnp::{
    call, cap_descriptor, disembargo, exception, message, message_target, payload, promised_answer,
    resolve, return_,
};
use crate::task_set::TaskSet;
use crate::{broken, local, queued};

pub type QuestionId = u32;
pub type AnswerId = QuestionId;
pub type ExportId = u32;
pub type ImportId = ExportId;

pub struct ImportTable<T> {
    slots: HashMap<u32, T>,
}

impl<T> ImportTable<T> {
    pub fn new() -> ImportTable<T> {
        ImportTable {
            slots: HashMap::new(),
        }
    }
}

#[derive(PartialEq, Eq)]
struct ReverseU32 {
    val: u32,
}

impl cmp::Ord for ReverseU32 {
    fn cmp(&self, other: &ReverseU32) -> cmp::Ordering {
        if self.val > other.val {
            cmp::Ordering::Less
        } else if self.val < other.val {
            cmp::Ordering::Greater
        } else {
            cmp::Ordering::Equal
        }
    }
}

impl cmp::PartialOrd for ReverseU32 {
    fn partial_cmp(&self, other: &ReverseU32) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct ExportTable<T> {
    slots: Vec<Option<T>>,

    // prioritize lower values
    free_ids: BinaryHeap<ReverseU32>,
}

struct ExportTableIter<'a, T>
where
    T: 'a,
{
    table: &'a ExportTable<T>,
    idx: usize,
}

impl<'a, T> ::std::iter::Iterator for ExportTableIter<'a, T>
where
    T: 'a,
{
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        while self.idx < self.table.slots.len() {
            let idx = self.idx;
            self.idx += 1;
            if let Some(ref v) = self.table.slots[idx] {
                return Some(v);
            }
        }
        None
    }
}

impl<T> ExportTable<T> {
    pub fn new() -> ExportTable<T> {
        ExportTable {
            slots: Vec::new(),
            free_ids: BinaryHeap::new(),
        }
    }

    pub fn erase(&mut self, id: u32) {
        self.slots[id as usize] = None;
        self.free_ids.push(ReverseU32 { val: id });
    }

    pub fn push(&mut self, val: T) -> u32 {
        match self.free_ids.pop() {
            Some(ReverseU32 { val: id }) => {
                self.slots[id as usize] = Some(val);
                id
            }
            None => {
                self.slots.push(Some(val));
                self.slots.len() as u32 - 1
            }
        }
    }

    pub fn find<'a>(&'a mut self, id: u32) -> Option<&'a mut T> {
        let idx = id as usize;
        if idx < self.slots.len() {
            match self.slots[idx] {
                Some(ref mut v) => Some(v),
                None => None,
            }
        } else {
            None
        }
    }

    pub fn iter<'a>(&'a self) -> ExportTableIter<'a, T> {
        ExportTableIter {
            table: self,
            idx: 0,
        }
    }
}

struct Question<VatId>
where
    VatId: 'static,
{
    is_awaiting_return: bool,
    param_exports: Vec<ExportId>,
    is_tail_call: bool,

    /// The local QuestionRef, set to None when it is destroyed.
    self_ref: Option<Weak<RefCell<QuestionRef<VatId>>>>,
}

impl<VatId> Question<VatId> {
    fn new() -> Question<VatId> {
        Question {
            is_awaiting_return: true,
            param_exports: Vec::new(),
            is_tail_call: false,
            self_ref: None,
        }
    }
}

/// A reference to an entry on the question table.  Used to detect when the `Finish` message
/// can be sent.
struct QuestionRef<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    id: QuestionId,
    fulfiller: Option<oneshot::Sender<Promise<Response<VatId>, Error>>>,
}

impl<VatId> QuestionRef<VatId> {
    fn new(
        state: Rc<ConnectionState<VatId>>,
        id: QuestionId,
        fulfiller: oneshot::Sender<Promise<Response<VatId>, Error>>,
    ) -> QuestionRef<VatId> {
        QuestionRef {
            connection_state: state,
            id: id,
            fulfiller: Some(fulfiller),
        }
    }
    fn fulfill(&mut self, response: Promise<Response<VatId>, Error>) {
        if let Some(fulfiller) = self.fulfiller.take() {
            let _ = fulfiller.send(response);
        }
    }

    fn reject(&mut self, err: Error) {
        if let Some(fulfiller) = self.fulfiller.take() {
            let _ = fulfiller.send(Promise::err(err));
        }
    }
}

impl<VatId> Drop for QuestionRef<VatId> {
    fn drop(&mut self) {
        let mut questions = self.connection_state.questions.borrow_mut();
        match questions.slots[self.id as usize] {
            Some(ref mut q) => {
                if let Ok(ref mut c) = *self.connection_state.connection.borrow_mut() {
                    let mut message = c.new_outgoing_message(100); // XXX size hint
                    {
                        let root: message::Builder = message.get_body().unwrap().init_as();
                        let mut builder = root.init_finish();
                        builder.set_question_id(self.id);

                        // If we're still awaiting a return, then this request is being
                        // canceled, and we're going to ignore any capabilities in the return
                        // message, so set releaseResultCaps true. If we already received the
                        // return, then we've already built local proxies for the caps and will
                        // send Release messages when those are destroyed.
                        builder.set_release_result_caps(q.is_awaiting_return);
                    }
                    let _ = message.send();
                }

                if q.is_awaiting_return {
                    // Still waiting for return, so just remove the QuestionRef pointer from the table.
                    q.self_ref = None;
                } else {
                    // Call has already returned, so we can now remove it from the table.
                    questions.erase(self.id)
                }
            }
            None => unreachable!(),
        }
    }
}

struct Answer<VatId>
where
    VatId: 'static,
{
    // True from the point when the Call message is received to the point when both the `Finish`
    // message has been received and the `Return` has been sent.
    active: bool,

    return_has_been_sent: bool,

    // Send pipelined calls here.  Becomes null as soon as a `Finish` is received.
    pipeline: Option<Box<dyn PipelineHook>>,

    // For locally-redirected calls (Call.sendResultsTo.yourself), this is a promise for the call
    // result, to be picked up by a subsequent `Return`.
    redirected_results: Option<Promise<Response<VatId>, Error>>,

    received_finish: Rc<Cell<bool>>,
    call_completion_promise: Option<Promise<(), Error>>,

    // List of exports that were sent in the results.  If the finish has `releaseResultCaps` these
    // will need to be released.
    result_exports: Vec<ExportId>,
}

impl<VatId> Answer<VatId> {
    fn new() -> Answer<VatId> {
        Answer {
            active: false,
            return_has_been_sent: false,
            pipeline: None,
            redirected_results: None,
            received_finish: Rc::new(Cell::new(false)),
            call_completion_promise: None,
            result_exports: Vec::new(),
        }
    }
}

pub struct Export {
    refcount: u32,
    client_hook: Box<dyn ClientHook>,

    // If this export is a promise (not a settled capability), the `resolve_op` represents the
    // ongoing operation to wait for that promise to resolve and then send a `Resolve` message.
    resolve_op: Promise<(), Error>,
}

impl Export {
    fn new(client_hook: Box<dyn ClientHook>) -> Export {
        Export {
            refcount: 1,
            client_hook: client_hook,
            resolve_op: Promise::err(Error::failed("no resolve op".to_string())),
        }
    }
}

pub struct Import<VatId>
where
    VatId: 'static,
{
    // Becomes null when the import is destroyed.
    import_client: Option<(Weak<RefCell<ImportClient<VatId>>>, usize)>,

    // Either a copy of importClient, or, in the case of promises, the wrapping PromiseClient.
    // Becomes null when it is discarded *or* when the import is destroyed (e.g. the promise is
    // resolved and the import is no longer needed).
    app_client: Option<WeakClient<VatId>>,

    // If non-null, the import is a promise.
    promise_client_to_resolve: Option<Weak<RefCell<PromiseClient<VatId>>>>,
}

impl<VatId> Import<VatId> {
    fn new() -> Import<VatId> {
        Import {
            import_client: None,
            app_client: None,
            promise_client_to_resolve: None,
        }
    }
}

struct Embargo {
    fulfiller: Option<oneshot::Sender<Result<(), Error>>>,
}

impl Embargo {
    fn new(fulfiller: oneshot::Sender<Result<(), Error>>) -> Embargo {
        Embargo {
            fulfiller: Some(fulfiller),
        }
    }
}

fn to_pipeline_ops(
    ops: ::capnp::struct_list::Reader<promised_answer::op::Owned>,
) -> ::capnp::Result<Vec<PipelineOp>> {
    let mut result = Vec::new();
    for op in ops.iter() {
        match op.which()? {
            promised_answer::op::Noop(()) => {
                result.push(PipelineOp::Noop);
            }
            promised_answer::op::GetPointerField(idx) => {
                result.push(PipelineOp::GetPointerField(idx));
            }
        }
    }
    Ok(result)
}

fn from_error(error: &Error, mut builder: exception::Builder) {
    builder.set_reason(&error.description);
    let typ = match error.kind {
        ::capnp::ErrorKind::Failed => exception::Type::Failed,
        ::capnp::ErrorKind::Overloaded => exception::Type::Overloaded,
        ::capnp::ErrorKind::Disconnected => exception::Type::Disconnected,
        ::capnp::ErrorKind::Unimplemented => exception::Type::Unimplemented,
    };
    builder.set_type(typ);
}

fn remote_exception_to_error(exception: exception::Reader) -> Error {
    let (kind, reason) = match (exception.get_type(), exception.get_reason()) {
        (Ok(exception::Type::Failed), Ok(reason)) => (::capnp::ErrorKind::Failed, reason),
        (Ok(exception::Type::Overloaded), Ok(reason)) => (::capnp::ErrorKind::Overloaded, reason),
        (Ok(exception::Type::Disconnected), Ok(reason)) => {
            (::capnp::ErrorKind::Disconnected, reason)
        }
        (Ok(exception::Type::Unimplemented), Ok(reason)) => {
            (::capnp::ErrorKind::Unimplemented, reason)
        }
        _ => (::capnp::ErrorKind::Failed, "(malformed error)"),
    };
    Error {
        description: format!("remote exception: {}", reason),
        kind: kind,
    }
}

pub struct ConnectionErrorHandler<VatId>
where
    VatId: 'static,
{
    weak_state: Weak<ConnectionState<VatId>>,
}

impl<VatId> ConnectionErrorHandler<VatId> {
    fn new(weak_state: Weak<ConnectionState<VatId>>) -> ConnectionErrorHandler<VatId> {
        ConnectionErrorHandler {
            weak_state: weak_state,
        }
    }
}

impl<VatId> crate::task_set::TaskReaper<capnp::Error> for ConnectionErrorHandler<VatId> {
    fn task_failed(&mut self, error: ::capnp::Error) {
        if let Some(state) = self.weak_state.upgrade() {
            state.disconnect(error)
        }
    }
}

pub struct ConnectionState<VatId>
where
    VatId: 'static,
{
    bootstrap_cap: Box<dyn ClientHook>,
    exports: RefCell<ExportTable<Export>>,
    questions: RefCell<ExportTable<Question<VatId>>>,
    answers: RefCell<ImportTable<Answer<VatId>>>,
    imports: RefCell<ImportTable<Import<VatId>>>,

    exports_by_cap: RefCell<HashMap<usize, ExportId>>,

    embargoes: RefCell<ExportTable<Embargo>>,

    tasks: RefCell<Option<crate::task_set::TaskSetHandle<capnp::Error>>>,
    connection: RefCell<::std::result::Result<Box<dyn crate::Connection<VatId>>, ::capnp::Error>>,
    disconnect_fulfiller: RefCell<Option<oneshot::Sender<Promise<(), Error>>>>,

    client_downcast_map: RefCell<HashMap<usize, WeakClient<VatId>>>,
}

impl<VatId> ConnectionState<VatId> {
    pub fn new(
        bootstrap_cap: Box<dyn ClientHook>,
        connection: Box<dyn crate::Connection<VatId>>,
        disconnect_fulfiller: oneshot::Sender<Promise<(), Error>>,
    ) -> (TaskSet<Error>, Rc<ConnectionState<VatId>>) {
        let state = Rc::new(ConnectionState {
            bootstrap_cap: bootstrap_cap,
            exports: RefCell::new(ExportTable::new()),
            questions: RefCell::new(ExportTable::new()),
            answers: RefCell::new(ImportTable::new()),
            imports: RefCell::new(ImportTable::new()),
            exports_by_cap: RefCell::new(HashMap::new()),
            embargoes: RefCell::new(ExportTable::new()),
            tasks: RefCell::new(None),
            connection: RefCell::new(Ok(connection)),
            disconnect_fulfiller: RefCell::new(Some(disconnect_fulfiller)),
            client_downcast_map: RefCell::new(HashMap::new()),
        });
        let (mut handle, tasks) =
            TaskSet::new(Box::new(ConnectionErrorHandler::new(Rc::downgrade(&state))));

        handle.add(ConnectionState::message_loop(Rc::downgrade(&state)));
        *state.tasks.borrow_mut() = Some(handle);
        (tasks, state)
    }

    fn new_outgoing_message(
        &self,
        first_segment_words: u32,
    ) -> capnp::Result<Box<dyn crate::OutgoingMessage>> {
        match self.connection.borrow_mut().as_mut() {
            Err(e) => Err(e.clone()),
            Ok(c) => Ok(c.new_outgoing_message(first_segment_words)),
        }
    }

    fn disconnect(&self, error: ::capnp::Error) {
        if self.connection.borrow().is_err() {
            // Already disconnected.
            return;
        }

        // Carefully pull all the objects out of the tables prior to releasing them because their
        // destructors could come back and mess with the tables.
        let mut pipelines_to_release = Vec::new();
        let mut clients_to_release = Vec::new();
        //let mut tail_calls_to_release = Vec::new();
        let mut resolve_ops_to_release = Vec::new();

        for q in self.questions.borrow().iter() {
            if let Some(ref weak_question_ref) = q.self_ref {
                if let Some(question_ref) = weak_question_ref.upgrade() {
                    question_ref.borrow_mut().reject(error.clone());
                }
            }
        }

        {
            let answer_slots = &mut self.answers.borrow_mut().slots;
            for (_, ref mut answer) in answer_slots.iter_mut() {
                // TODO tail call
                pipelines_to_release.push(answer.pipeline.take())
            }
        }

        let len = self.exports.borrow().slots.len();
        for idx in 0..len {
            if let Some(exp) = self.exports.borrow_mut().slots[idx].take() {
                let Export {
                    client_hook,
                    resolve_op,
                    ..
                } = exp;
                clients_to_release.push(client_hook);
                resolve_ops_to_release.push(resolve_op);
            }
        }
        *self.exports.borrow_mut() = ExportTable::new();

        {
            let import_slots = &mut self.imports.borrow_mut().slots;
            for (_, ref mut import) in import_slots.iter_mut() {
                if let Some(f) = import.promise_client_to_resolve.take() {
                    if let Some(promise_client) = f.upgrade() {
                        promise_client.borrow_mut().resolve(Err(error.clone()));
                    }
                }
            }
        }

        let len = self.embargoes.borrow().slots.len();
        for idx in 0..len {
            if let Some(ref mut emb) = self.embargoes.borrow_mut().slots[idx] {
                if let Some(f) = emb.fulfiller.take() {
                    let _ = f.send(Err(error.clone()));
                }
            }
        }
        *self.embargoes.borrow_mut() = ExportTable::new();

        drop(pipelines_to_release);
        drop(clients_to_release);
        drop(resolve_ops_to_release);
        // TODO drop tail calls

        match *self.connection.borrow_mut() {
            Ok(ref mut c) => {
                let mut message = c.new_outgoing_message(100); // TODO estimate size
                {
                    let builder = message
                        .get_body()
                        .unwrap()
                        .init_as::<message::Builder>()
                        .init_abort();
                    from_error(&error, builder);
                }
                let _ = message.send();
            }
            Err(_) => unreachable!(),
        }

        let connection = mem::replace(&mut *self.connection.borrow_mut(), Err(error.clone()));

        match connection {
            Ok(mut c) => {
                let promise = c.shutdown(Err(error)).then(|r| match r {
                    Ok(()) => Promise::ok(()),
                    Err(e) => {
                        if e.kind != ::capnp::ErrorKind::Disconnected {
                            // Don't report disconnects as an error.
                            Promise::err(e)
                        } else {
                            Promise::ok(())
                        }
                    }
                });
                let maybe_fulfiller =
                    mem::replace(&mut *self.disconnect_fulfiller.borrow_mut(), None);
                match maybe_fulfiller {
                    None => unreachable!(),
                    Some(fulfiller) => {
                        let _ = fulfiller.send(Promise::from_future(promise.attach(c)));
                    }
                }
            }
            Err(_) => unreachable!(),
        }
    }

    // Transform a future into a promise that gets executed even if it is never polled.
    // Dropping the returned promise cancels the computation.
    fn eagerly_evaluate<T, F>(&self, task: F) -> Promise<T, Error>
    where
        F: Future<Output = Result<T, Error>> + 'static + Unpin,
        T: 'static,
    {
        let (tx, rx) = oneshot::channel::<Result<T, Error>>();
        let (tx2, rx2) = oneshot::channel::<()>();
        let f1 = Box::pin(task.map(move |r| {
            let _ = tx.send(r);
        })) as Pin<Box<dyn Future<Output = ()> + Unpin>>;
        let f2 = Box::pin(rx2.map(drop)) as Pin<Box<dyn Future<Output = ()> + Unpin>>;

        self.add_task(future::select(f1, f2).map(|_| Ok(())));
        Promise::from_future(rx.map_err(crate::canceled_to_error).map(|r| {
            drop(tx2);
            r?
        }))
    }

    fn add_task<F>(&self, task: F)
    where
        F: Future<Output = Result<(), Error>> + 'static,
    {
        if let Some(ref mut tasks) = *self.tasks.borrow_mut() {
            tasks.add(task);
        }
    }

    pub fn bootstrap(state: Rc<ConnectionState<VatId>>) -> Box<dyn ClientHook> {
        let question_id = state.questions.borrow_mut().push(Question::new());

        let (fulfiller, promise) = oneshot::channel();
        let promise = promise.map_err(crate::canceled_to_error);
        let promise = promise.and_then(|response_promise| response_promise);
        let question_ref = Rc::new(RefCell::new(QuestionRef::new(
            state.clone(),
            question_id,
            fulfiller,
        )));
        let promise = promise.attach(question_ref.clone());
        match state.questions.borrow_mut().slots[question_id as usize] {
            Some(ref mut q) => {
                q.self_ref = Some(Rc::downgrade(&question_ref));
            }
            None => unreachable!(),
        }
        match *state.connection.borrow_mut() {
            Ok(ref mut c) => {
                let mut message = c.new_outgoing_message(100); // TODO estimate size
                {
                    let mut builder = message
                        .get_body()
                        .unwrap()
                        .init_as::<message::Builder>()
                        .init_bootstrap();
                    builder.set_question_id(question_id);
                }
                let _ = message.send();
            }
            Err(_) => panic!(),
        }

        let pipeline = Pipeline::new(state, question_ref, Some(Promise::from_future(promise)));
        pipeline.get_pipelined_cap_move(Vec::new())
    }

    fn message_loop(weak_state: Weak<ConnectionState<VatId>>) -> Promise<(), capnp::Error> {
        let state = match weak_state.upgrade() {
            None => {
                return Promise::err(Error::disconnected(
                    "message loop cannot continue without a connection".into(),
                ))
            }
            Some(c) => c,
        };

        let promise = match *state.connection.borrow_mut() {
            Err(_) => return Promise::ok(()),
            Ok(ref mut connection) => connection.receive_incoming_message(),
        };

        Promise::from_future(async move {
            match promise.await? {
                Some(m) => {
                    ConnectionState::handle_message(weak_state.clone(), m)?;
                    weak_state
                        .upgrade()
                        .expect("message loop outlived connection state?")
                        .add_task(ConnectionState::message_loop(weak_state));
                }
                None => {
                    weak_state
                        .upgrade()
                        .expect("message loop outlived connection state?")
                        .disconnect(Error::disconnected("Peer disconnected.".to_string()));
                }
            }
            Ok(())
        })
    }

    fn send_unimplemented(
        connection_state: Rc<ConnectionState<VatId>>,
        message: Box<dyn crate::IncomingMessage>,
    ) -> capnp::Result<()> {
        let mut out_message = connection_state.new_outgoing_message(50)?; // XXX size hint
        {
            let mut root: message::Builder = out_message.get_body()?.get_as()?;
            root.set_unimplemented(message.get_body()?.get_as()?)?;
        }
        let _ = out_message.send();
        Ok(())
    }

    fn handle_message(
        weak_state: Weak<ConnectionState<VatId>>,
        message: Box<dyn crate::IncomingMessage>,
    ) -> ::capnp::Result<()> {
        let connection_state = match weak_state.upgrade() {
            None => {
                return Err(Error::disconnected(
                    "handle_message() cannot continue without a connection".into(),
                ))
            }
            Some(c) => c,
        };

        let reader = message.get_body()?.get_as::<message::Reader>()?;
        match reader.which() {
            Ok(message::Unimplemented(message)) => {
                let message = message?;
                match message.which()? {
                    message::Resolve(resolve) => {
                        let resolve = resolve?;
                        match resolve.which()? {
                            resolve::Cap(c) => match c?.which()? {
                                cap_descriptor::None(()) => (),
                                cap_descriptor::SenderHosted(export_id) => {
                                    connection_state.release_export(export_id, 1)?;
                                }
                                cap_descriptor::SenderPromise(export_id) => {
                                    connection_state.release_export(export_id, 1)?;
                                }
                                cap_descriptor::ReceiverAnswer(_)
                                | cap_descriptor::ReceiverHosted(_) => (),
                                cap_descriptor::ThirdPartyHosted(_) => {
                                    return Err(Error::failed(
                                        "Peer claims we resolved a ThirdPartyHosted cap."
                                            .to_string(),
                                    ));
                                }
                            },
                            resolve::Exception(_) => (),
                        }
                    }
                    _ => {
                        return Err(Error::failed(
                            "Peer did not implement required RPC message type.".to_string(),
                        ));
                    }
                }
            }
            Ok(message::Abort(abort)) => return Err(remote_exception_to_error(abort?)),
            Ok(message::Bootstrap(bootstrap)) => {
                use ::capnp::traits::ImbueMut;

                let bootstrap = bootstrap?;
                let answer_id = bootstrap.get_question_id();

                if connection_state.connection.borrow().is_err() {
                    // Disconnected; ignore.
                    return Ok(());
                }

                let mut response = connection_state.new_outgoing_message(50)?; // XXX size hint

                let result_exports = {
                    let mut ret = response
                        .get_body()?
                        .init_as::<message::Builder>()
                        .init_return();
                    ret.set_answer_id(answer_id);

                    let cap = connection_state.bootstrap_cap.clone();
                    let mut cap_table = Vec::new();
                    let mut payload = ret.init_results();
                    {
                        let mut content = payload.reborrow().get_content();
                        content.imbue_mut(&mut cap_table);
                        content.set_as_capability(cap);
                    }
                    assert_eq!(cap_table.len(), 1);

                    ConnectionState::write_descriptors(&connection_state, &cap_table, payload)
                };

                let slots = &mut connection_state.answers.borrow_mut().slots;
                let answer = slots.entry(answer_id).or_insert_with(Answer::new);
                if answer.active {
                    connection_state.release_exports(&result_exports)?;
                    return Err(Error::failed("questionId is already in use".to_string()));
                }
                answer.active = true;
                answer.return_has_been_sent = true;
                answer.result_exports = result_exports;
                answer.pipeline = Some(Box::new(SingleCapPipeline::new(
                    connection_state.bootstrap_cap.clone(),
                )));

                let _ = response.send();
            }
            Ok(message::Call(call)) => {
                let capability = connection_state.get_message_target(call?.get_target()?)?;
                let (interface_id, method_id, question_id, cap_table_array, redirect_results) = {
                    let call = match message.get_body()?.get_as::<message::Reader>()?.which()? {
                        message::Call(call) => call?,
                        _ => {
                            // exception already reported?
                            unreachable!()
                        }
                    };
                    let redirect_results = match call.get_send_results_to().which()? {
                        call::send_results_to::Caller(()) => false,
                        call::send_results_to::Yourself(()) => true,
                        call::send_results_to::ThirdParty(_) => {
                            return Err(Error::failed(
                                "Unsupported `Call.sendResultsTo`.".to_string(),
                            ))
                        }
                    };
                    let payload = call.get_params()?;

                    (
                        call.get_interface_id(),
                        call.get_method_id(),
                        call.get_question_id(),
                        ConnectionState::receive_caps(
                            connection_state.clone(),
                            payload.get_cap_table()?,
                        )?,
                        redirect_results,
                    )
                };

                if connection_state
                    .answers
                    .borrow()
                    .slots
                    .contains_key(&question_id)
                {
                    return Err(Error::failed(format!(
                        "Received a new call on in-use question id {}",
                        question_id
                    )));
                }

                let params = Params::new(message, cap_table_array);

                let answer = Answer::new();

                let (results_inner_fulfiller, results_inner_promise) = oneshot::channel();
                let results_inner_promise = results_inner_promise.map_err(crate::canceled_to_error);
                let results = Results::new(
                    &connection_state,
                    question_id,
                    redirect_results,
                    results_inner_fulfiller,
                    answer.received_finish.clone(),
                );

                let (redirected_results_done_promise, redirected_results_done_fulfiller) =
                    if redirect_results {
                        let (f, p) = oneshot::channel::<Result<Response<VatId>, Error>>();
                        let p = p
                            .map_err(crate::canceled_to_error)
                            .and_then(|x| future::ready(x));
                        (Some(Promise::from_future(p)), Some(f))
                    } else {
                        (None, None)
                    };

                {
                    let ref mut slots = connection_state.answers.borrow_mut().slots;
                    let answer = slots.entry(question_id).or_insert(answer);
                    if answer.active {
                        return Err(Error::failed("questionId is already in use".to_string()));
                    }
                    answer.active = true;
                }

                let call_promise =
                    capability.call(interface_id, method_id, Box::new(params), Box::new(results));
                let (pipeline_sender, mut pipeline) = queued::Pipeline::new();

                let promise = call_promise
                    .then(move |call_result| {
                        results_inner_promise.then(move |result| {
                            future::ready(ResultsDone::from_results_inner(
                                result,
                                call_result,
                                pipeline_sender,
                            ))
                        })
                    })
                    .then(move |v| {
                        match redirected_results_done_fulfiller {
                            Some(f) => match v {
                                Ok(ref r) => drop(f.send(Ok(Response::redirected(r.clone())))),
                                Err(ref e) => drop(f.send(Err(e.clone()))),
                            },
                            None => (),
                        }
                        Promise::ok(())
                    });

                let fork = promise.shared();
                pipeline.drive(fork.clone());

                {
                    let ref mut slots = connection_state.answers.borrow_mut().slots;
                    match slots.get_mut(&question_id) {
                        Some(ref mut answer) => {
                            answer.pipeline = Some(Box::new(pipeline));
                            if redirect_results {
                                answer.redirected_results = redirected_results_done_promise;
                            // More to do here?
                            } else {
                                answer.call_completion_promise =
                                    Some(connection_state.eagerly_evaluate(fork));
                            }
                        }
                        None => unreachable!(),
                    }
                }
            }
            Ok(message::Return(oret)) => {
                let ret = oret?;
                let question_id = ret.get_answer_id();

                let mut questions = connection_state.questions.borrow_mut();
                match questions.slots[question_id as usize] {
                    Some(ref mut question) => {
                        question.is_awaiting_return = false;
                        match question.self_ref {
                            Some(ref question_ref) => match ret.which()? {
                                return_::Results(results) => {
                                    let cap_table = ConnectionState::receive_caps(
                                        connection_state.clone(),
                                        results?.get_cap_table()?,
                                    )?;

                                    let question_ref =
                                        question_ref.upgrade().expect("dangling question ref?");
                                    let response = Response::new(
                                        connection_state.clone(),
                                        question_ref.clone(),
                                        message,
                                        cap_table,
                                    );
                                    question_ref.borrow_mut().fulfill(Promise::ok(response));
                                }
                                return_::Exception(e) => {
                                    let tmp =
                                        question_ref.upgrade().expect("dangling question ref?");
                                    tmp.borrow_mut().reject(remote_exception_to_error(e?));
                                }
                                return_::Canceled(_) => unimplemented!(),
                                return_::ResultsSentElsewhere(_) => unimplemented!(),
                                return_::TakeFromOtherQuestion(id) => {
                                    if let Some(ref mut answer) =
                                        connection_state.answers.borrow_mut().slots.get_mut(&id)
                                    {
                                        if let Some(res) = answer.redirected_results.take() {
                                            let tmp = question_ref
                                                .upgrade()
                                                .expect("dangling question ref?");
                                            tmp.borrow_mut().fulfill(res);
                                        } else {
                                            return Err(Error::failed(format!(
                                                    "return.takeFromOtherQuestion referenced a call that \
                                                     did not use sendResultsTo.yourself.")));
                                        }
                                    } else {
                                        return Err(Error::failed(format!(
                                            "return.takeFromOtherQuestion had invalid answer ID."
                                        )));
                                    }
                                }
                                return_::AcceptFromThirdParty(_) => {
                                    drop(questions);
                                    ConnectionState::send_unimplemented(connection_state, message)?;
                                }
                            },
                            None => {
                                match ret.which()? {
                                    return_::TakeFromOtherQuestion(_) => unimplemented!(),
                                    _ => {}
                                }
                                // Looks like this question was canceled earlier, so `Finish`
                                // was already sent, with `releaseResultCaps` set true so that
                                // we don't have to release them here. We can go ahead and
                                // delete it from the table.
                                questions.erase(question_id);
                            }
                        }
                    }
                    None => {
                        return Err(Error::failed(format!(
                            "Invalid question ID in Return message: {}",
                            question_id
                        )));
                    }
                }
            }
            Ok(message::Finish(finish)) => {
                let finish = finish?;

                let mut exports_to_release = Vec::new();
                let answer_id = finish.get_question_id();

                let mut erase = false;
                let answers_slots = &mut connection_state.answers.borrow_mut().slots;
                match answers_slots.get_mut(&answer_id) {
                    None => {
                        return Err(Error::failed(format!(
                            "Invalid question ID {} in Finish message.",
                            answer_id
                        )));
                    }
                    Some(ref mut answer) => {
                        if !answer.active {
                            return Err(Error::failed(format!(
                                "'Finish' for invalid question ID {}.",
                                answer_id
                            )));
                        }
                        answer.received_finish.set(true);

                        if finish.get_release_result_caps() {
                            exports_to_release =
                                mem::replace(&mut answer.result_exports, Vec::new());
                        }

                        // If the pipeline has not been cloned, the following two lines cancel the call.
                        answer.pipeline.take();
                        answer.call_completion_promise.take();

                        if answer.return_has_been_sent {
                            erase = true;
                        }
                    }
                }

                if erase {
                    answers_slots.remove(&answer_id);
                }

                connection_state.release_exports(&exports_to_release)?;
            }
            Ok(message::Resolve(resolve)) => {
                let resolve = resolve?;
                let replacement_or_error = match resolve.which()? {
                    resolve::Cap(c) => {
                        match ConnectionState::receive_cap(connection_state.clone(), c?)? {
                            Some(cap) => Ok(cap),
                            None => {
                                return Err(Error::failed(format!(
                                    "'Resolve' contained 'CapDescriptor.none'."
                                )));
                            }
                        }
                    }
                    resolve::Exception(e) => {
                        // We can't set `replacement` to a new broken cap here because this will
                        // confuse PromiseClient::Resolve() into thinking that the remote
                        // promise resolved to a local capability and therefore a Disembargo is
                        // needed. We must actually reject the promise.
                        Err(remote_exception_to_error(e?))
                    }
                };

                // If the import is in the table, fulfill it.
                let ref mut slots = connection_state.imports.borrow_mut().slots;
                if let Some(ref mut import) = slots.get_mut(&resolve.get_promise_id()) {
                    match import.promise_client_to_resolve.take() {
                        Some(weak_promise_client) => {
                            match weak_promise_client.upgrade() {
                                Some(promise_client) => {
                                    promise_client.borrow_mut().resolve(replacement_or_error);
                                }
                                None => {
                                    // ?
                                }
                            }
                        }
                        None => {
                            return Err(Error::failed(format!(
                                "Got 'Resolve' for a non-promise import."
                            )));
                        }
                    }
                }
            }
            Ok(message::Release(release)) => {
                let release = release?;
                connection_state.release_export(release.get_id(), release.get_reference_count())?;
            }
            Ok(message::Disembargo(disembargo)) => {
                let disembargo = disembargo?;
                let context = disembargo.get_context();
                match context.which()? {
                    disembargo::context::SenderLoopback(embargo_id) => {
                        let mut target =
                            connection_state.get_message_target(disembargo.get_target()?)?;
                        while let Some(resolved) = target.get_resolved() {
                            target = resolved;
                        }

                        if target.get_brand() != connection_state.get_brand() {
                            return Err(Error::failed(
                                "'Disembargo' of type 'senderLoopback' sent to an object that does not point \
                                 back to the sender.".to_string()));
                        }

                        let connection_state_ref = connection_state.clone();
                        let connection_state_ref1 = connection_state.clone();
                        let task = async move {
                            if let Ok(ref mut c) = *connection_state_ref.connection.borrow_mut() {
                                let mut message = c.new_outgoing_message(100); // TODO estimate size
                                {
                                    let root: message::Builder = message.get_body()?.init_as();
                                    let mut disembargo = root.init_disembargo();
                                    disembargo
                                        .reborrow()
                                        .init_context()
                                        .set_receiver_loopback(embargo_id);

                                    let redirect = match Client::from_ptr(
                                        target.get_ptr(),
                                        &connection_state_ref1,
                                    ) {
                                        Some(c) => c.write_target(disembargo.init_target()),
                                        None => unreachable!(),
                                    };
                                    if redirect.is_some() {
                                        return Err(Error::failed(
                                            "'Disembargo' of type 'senderLoopback' sent to an object that \
                                             does not appear to have been the subject of a previous \
                                             'Resolve' message.".to_string()));
                                    }
                                }
                                let _ = message.send();
                            }
                            Ok(())
                        };
                        connection_state.add_task(task);
                    }
                    disembargo::context::ReceiverLoopback(embargo_id) => {
                        if let Some(ref mut embargo) =
                            connection_state.embargoes.borrow_mut().find(embargo_id)
                        {
                            let fulfiller = embargo.fulfiller.take().unwrap();
                            let _ = fulfiller.send(Ok(()));
                        } else {
                            return Err(Error::failed(
                                "Invalid embargo ID in `Disembargo.context.receiverLoopback"
                                    .to_string(),
                            ));
                        }
                        connection_state.embargoes.borrow_mut().erase(embargo_id);
                    }
                    disembargo::context::Accept(_) | disembargo::context::Provide(_) => {
                        return Err(Error::unimplemented(
                            "Disembargo::Context::Provide/Accept not implemented".to_string(),
                        ));
                    }
                }
            }
            Ok(message::Provide(_))
            | Ok(message::Accept(_))
            | Ok(message::Join(_))
            | Ok(message::ObsoleteSave(_))
            | Ok(message::ObsoleteDelete(_))
            | Err(::capnp::NotInSchema(_)) => {
                ConnectionState::send_unimplemented(connection_state, message)?;
            }
        }
        Ok(())
    }

    fn answer_has_sent_return(&self, id: AnswerId, result_exports: Vec<ExportId>) {
        let mut erase = false;
        let answers_slots = &mut self.answers.borrow_mut().slots;
        if let Some(ref mut a) = answers_slots.get_mut(&id) {
            a.return_has_been_sent = true;
            if a.received_finish.get() {
                erase = true;
            } else {
                a.result_exports = result_exports;
            }
        } else {
            unreachable!()
        }

        if erase {
            answers_slots.remove(&id);
        }
    }

    fn release_export(&self, id: ExportId, refcount: u32) -> ::capnp::Result<()> {
        let mut erase_export = false;
        let mut client_ptr = 0;
        match self.exports.borrow_mut().find(id) {
            Some(e) => {
                if refcount > e.refcount {
                    return Err(Error::failed(
                        "Tried to drop export's refcount below zero.".to_string(),
                    ));
                } else {
                    e.refcount -= refcount;
                    if e.refcount == 0 {
                        erase_export = true;
                        client_ptr = e.client_hook.get_ptr();
                    }
                }
            }
            None => {
                return Err(Error::failed(
                    "Tried to release invalid export ID.".to_string(),
                ));
            }
        }
        if erase_export {
            self.exports.borrow_mut().erase(id);
            self.exports_by_cap.borrow_mut().remove(&client_ptr);
        }
        Ok(())
    }

    fn release_exports(&self, exports: &[ExportId]) -> ::capnp::Result<()> {
        for &export_id in exports {
            self.release_export(export_id, 1)?;
        }
        Ok(())
    }

    fn get_brand(&self) -> usize {
        self as *const _ as usize
    }

    fn get_message_target(
        &self,
        target: message_target::Reader,
    ) -> ::capnp::Result<Box<dyn ClientHook>> {
        match target.which()? {
            message_target::ImportedCap(export_id) => {
                match self.exports.borrow().slots.get(export_id as usize) {
                    Some(&Some(ref exp)) => Ok(exp.client_hook.clone()),
                    _ => Err(Error::failed(
                        "Message target is not a current export ID.".to_string(),
                    )),
                }
            }
            message_target::PromisedAnswer(promised_answer) => {
                let promised_answer = promised_answer?;
                let question_id = promised_answer.get_question_id();

                match self.answers.borrow().slots.get(&question_id) {
                    None => Err(Error::failed(
                        "PromisedAnswer.questionId is not a current question.".to_string(),
                    )),
                    Some(base) => {
                        let pipeline = match base.pipeline {
                            Some(ref pipeline) => pipeline.add_ref(),
                            None => Box::new(broken::Pipeline::new(Error::failed(
                                "Pipeline call on a request that returned not capabilities or was \
                                 already closed."
                                    .to_string(),
                            ))) as Box<dyn PipelineHook>,
                        };
                        let ops = to_pipeline_ops(promised_answer.get_transform()?)?;
                        Ok(pipeline.get_pipelined_cap(&ops))
                    }
                }
            }
        }
    }

    /// If calls to the given capability should pass over this connection, fill in `target`
    /// appropriately for such a call and return nullptr.  Otherwise, return a `ClientHook` to which
    /// the call should be forwarded; the caller should then delegate the call to that `ClientHook`.
    ///
    /// The main case where this ends up returning non-null is if `cap` is a promise that has
    /// recently resolved.  The application might have started building a request before the promise
    /// resolved, and so the request may have been built on the assumption that it would be sent over
    /// this network connection, but then the promise resolved to point somewhere else before the
    /// request was sent.  Now the request has to be redirected to the new target instead.
    fn write_target(
        &self,
        cap: &dyn ClientHook,
        target: message_target::Builder,
    ) -> Option<Box<dyn ClientHook>> {
        if cap.get_brand() == self.get_brand() {
            match Client::from_ptr(cap.get_ptr(), self) {
                Some(c) => c.write_target(target),
                None => unreachable!(),
            }
        } else {
            Some(cap.add_ref())
        }
    }

    fn get_innermost_client(&self, client_ref: &Box<dyn ClientHook>) -> Box<dyn ClientHook> {
        let mut client = client_ref.clone();
        while let Some(inner) = client.get_resolved() {
            client = inner;
        }
        if client.get_brand() == self.get_brand() {
            match self.client_downcast_map.borrow().get(&client.get_ptr()) {
                Some(c) => Box::new(c.upgrade().expect("dangling client?")),
                None => unreachable!(),
            }
        } else {
            client
        }
    }

    /// Implements exporting of a promise.  The promise has been exported under the given ID, and is
    /// to eventually resolve to the ClientHook produced by `promise`.  This method waits for that
    /// resolve to happen and then sends the appropriate `Resolve` message to the peer.
    fn resolve_exported_promise(
        state: &Rc<ConnectionState<VatId>>,
        export_id: ExportId,
        promise: Promise<Box<dyn ClientHook>, Error>,
    ) -> Promise<(), Error> {
        let weak_connection_state = Rc::downgrade(state);
        state.eagerly_evaluate(promise.map(move |resolution_result| {
            let connection_state = weak_connection_state
                .upgrade()
                .expect("dangling connection state?");

            match resolution_result {
                Ok(resolution) => {
                    let resolution = connection_state.get_innermost_client(&resolution);

                    let brand = resolution.get_brand();

                    // Update the export table to point at this object instead. We know that our
                    // entry in the export table is still live because when it is destroyed the
                    // asynchronous resolution task (i.e. this code) is canceled.
                    if let Some(ref mut exp) = connection_state.exports.borrow_mut().find(export_id)
                    {
                        connection_state
                            .exports_by_cap
                            .borrow_mut()
                            .remove(&exp.client_hook.get_ptr());
                        exp.client_hook = resolution.clone();
                    } else {
                        return Err(Error::failed("export table entry not found".to_string()));
                    }

                    if brand != connection_state.get_brand() {
                        // We're resolving to a local capability. If we're resolving to a promise,
                        // we might be able to reuse our export table entry and avoid sending a
                        // message.
                        if let Some(_promise) = resolution.when_more_resolved() {
                            // We're replacing a promise with another local promise. In this case,
                            // we might actually be able to just reuse the existing export table
                            // entry to represent the new promise -- unless it already has an entry.
                            // Let's check.

                            unimplemented!()
                        }
                    }

                    // OK, we have to send a `Resolve` message.
                    let mut message = connection_state.new_outgoing_message(100)?; // XXX size hint?
                    {
                        let root: message::Builder = message.get_body()?.get_as()?;
                        let mut resolve = root.init_resolve();
                        resolve.set_promise_id(export_id);
                        let _export = ConnectionState::write_descriptor(
                            &connection_state,
                            &resolution,
                            resolve.init_cap(),
                        )?;
                    }
                    let _ = message.send();
                    Ok(())
                }
                Err(e) => {
                    // send error resolution
                    let mut message = connection_state.new_outgoing_message(100)?; // XXX size hint?
                    {
                        let root: message::Builder = message.get_body()?.get_as()?;
                        let mut resolve = root.init_resolve();
                        resolve.set_promise_id(export_id);
                        from_error(&e, resolve.init_exception());
                    }
                    let _ = message.send();
                    Ok(())
                }
            }
        }))
    }

    fn write_descriptor(
        state: &Rc<ConnectionState<VatId>>,
        cap: &Box<dyn ClientHook>,
        mut descriptor: cap_descriptor::Builder,
    ) -> ::capnp::Result<Option<ExportId>> {
        // Find the innermost wrapped capability.
        let mut inner = cap.clone();
        while let Some(resolved) = inner.get_resolved() {
            inner = resolved;
        }
        if inner.get_brand() == state.get_brand() {
            let result = match Client::from_ptr(inner.get_ptr(), state) {
                Some(c) => c.write_descriptor(descriptor),
                None => unreachable!(),
            };
            Ok(result)
        } else {
            let ptr = inner.get_ptr();
            let contains_key = state.exports_by_cap.borrow().contains_key(&ptr);
            if contains_key {
                // We've already seen and exported this capability before.  Just up the refcount.
                let export_id = state.exports_by_cap.borrow()[(&ptr)];
                match state.exports.borrow_mut().find(export_id) {
                    None => unreachable!(),
                    Some(ref mut exp) => {
                        descriptor.set_sender_hosted(export_id);
                        exp.refcount += 1;
                        Ok(Some(export_id))
                    }
                }
            } else {
                // This is the first time we've seen this capability.

                let exp = Export::new(inner.clone());
                let export_id = state.exports.borrow_mut().push(exp);
                state.exports_by_cap.borrow_mut().insert(ptr, export_id);
                match inner.when_more_resolved() {
                    Some(wrapped) => {
                        // This is a promise.  Arrange for the `Resolve` message to be sent later.
                        if let Some(ref mut exp) = state.exports.borrow_mut().find(export_id) {
                            exp.resolve_op = ConnectionState::resolve_exported_promise(
                                state, export_id, wrapped,
                            );
                        }
                        descriptor.set_sender_promise(export_id);
                    }
                    None => {
                        descriptor.set_sender_hosted(export_id);
                    }
                }
                Ok(Some(export_id))
            }
        }
    }

    fn write_descriptors(
        state: &Rc<ConnectionState<VatId>>,
        cap_table: &[Option<Box<dyn ClientHook>>],
        payload: payload::Builder,
    ) -> Vec<ExportId> {
        let mut cap_table_builder = payload.init_cap_table(cap_table.len() as u32);
        let mut exports = Vec::new();
        for idx in 0..cap_table.len() {
            match cap_table[idx] {
                Some(ref cap) => {
                    match ConnectionState::write_descriptor(
                        state,
                        cap,
                        cap_table_builder.reborrow().get(idx as u32),
                    )
                    .unwrap()
                    {
                        Some(export_id) => {
                            exports.push(export_id);
                        }
                        None => {}
                    }
                }
                None => {
                    cap_table_builder.reborrow().get(idx as u32).set_none(());
                }
            }
        }
        exports
    }

    fn import(
        state: Rc<ConnectionState<VatId>>,
        import_id: ImportId,
        is_promise: bool,
    ) -> Box<dyn ClientHook> {
        let connection_state = state.clone();

        let import_client = {
            let slots = &mut state.imports.borrow_mut().slots;
            let v = slots.entry(import_id).or_insert_with(Import::new);
            if v.import_client.is_some() {
                v.import_client
                    .as_ref()
                    .unwrap()
                    .0
                    .upgrade()
                    .expect("dangling ref to import client?")
                    .clone()
            } else {
                let import_client = ImportClient::new(&connection_state, import_id);
                v.import_client = Some((
                    Rc::downgrade(&import_client),
                    (&*import_client.borrow()) as *const _ as usize,
                ));
                import_client
            }
        };

        // We just received a copy of this import ID, so the remote refcount has gone up.
        import_client.borrow_mut().add_remote_ref();

        if is_promise {
            // We need to construct a PromiseClient around this import, if we haven't already.
            match state.imports.borrow_mut().slots.get_mut(&import_id) {
                Some(ref mut import) => {
                    match import.app_client {
                        Some(ref c) => {
                            // Use the existing one.
                            Box::new(c.upgrade().expect("dangling client ref?"))
                        }
                        None => {
                            // Create a promise for this import's resolution.

                            let client: Box<Client<VatId>> = Box::new(import_client.into());
                            let client: Box<dyn ClientHook> = client;

                            // XXX do I need something like this?
                            // Make sure the import is not destroyed while this promise exists.
                            //                            let promise = promise.attach(client.add_ref());

                            let client =
                                PromiseClient::new(&connection_state, client, Some(import_id));

                            import.promise_client_to_resolve = Some(Rc::downgrade(&client));
                            let client: Box<Client<VatId>> = Box::new(client.into());
                            import.app_client = Some(client.downgrade());
                            client
                        }
                    }
                }
                None => unreachable!(),
            }
        } else {
            let client: Box<Client<VatId>> = Box::new(import_client.into());
            match state.imports.borrow_mut().slots.get_mut(&import_id) {
                Some(ref mut v) => {
                    v.app_client = Some(client.downgrade());
                }
                None => unreachable!(),
            };

            client
        }
    }

    fn receive_cap(
        state: Rc<ConnectionState<VatId>>,
        descriptor: cap_descriptor::Reader,
    ) -> ::capnp::Result<Option<Box<dyn ClientHook>>> {
        match descriptor.which()? {
            cap_descriptor::None(()) => Ok(None),
            cap_descriptor::SenderHosted(sender_hosted) => {
                Ok(Some(ConnectionState::import(state, sender_hosted, false)))
            }
            cap_descriptor::SenderPromise(sender_promise) => {
                Ok(Some(ConnectionState::import(state, sender_promise, true)))
            }
            cap_descriptor::ReceiverHosted(receiver_hosted) => {
                if let Some(ref mut exp) = state.exports.borrow_mut().find(receiver_hosted) {
                    Ok(Some(exp.client_hook.add_ref()))
                } else {
                    Ok(Some(broken::new_cap(Error::failed(
                        "invalid 'receivedHosted' export ID".to_string(),
                    ))))
                }
            }
            cap_descriptor::ReceiverAnswer(receiver_answer) => {
                let promised_answer = receiver_answer?;
                let question_id = promised_answer.get_question_id();
                match state.answers.borrow().slots.get(&question_id) {
                    Some(answer) => {
                        if answer.active {
                            match answer.pipeline {
                                Some(ref pipeline) => {
                                    let ops = to_pipeline_ops(promised_answer.get_transform()?)?;
                                    return Ok(Some(pipeline.get_pipelined_cap(&ops)));
                                }
                                None => (),
                            }
                        }
                    }
                    None => (),
                }
                Ok(Some(broken::new_cap(Error::failed(
                    "invalid 'receiver answer'".to_string(),
                ))))
            }
            cap_descriptor::ThirdPartyHosted(_third_party_hosted) => Err(Error::unimplemented(
                "ThirdPartyHosted caps are not supported.".to_string(),
            )),
        }
    }

    fn receive_caps(
        state: Rc<ConnectionState<VatId>>,
        cap_table: ::capnp::struct_list::Reader<cap_descriptor::Owned>,
    ) -> ::capnp::Result<Vec<Option<Box<dyn ClientHook>>>> {
        let mut result = Vec::new();
        for idx in 0..cap_table.len() {
            result.push(ConnectionState::receive_cap(
                state.clone(),
                cap_table.get(idx),
            )?);
        }
        Ok(result)
    }
}

enum DisconnectorState {
    Connected,
    Disconnecting,
    Disconnected,
}

/// A `Future` that can be run to disconnect an `RpcSystem`'s ConnectionState and wait for it to be closed.
pub struct Disconnector<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<RefCell<Option<Rc<ConnectionState<VatId>>>>>,
    state: DisconnectorState,
}

impl<VatId> Disconnector<VatId> {
    pub fn new(
        connection_state: Rc<RefCell<Option<Rc<ConnectionState<VatId>>>>>,
    ) -> Disconnector<VatId> {
        let state = match *(connection_state.borrow()) {
            Some(_) => DisconnectorState::Connected,
            None => DisconnectorState::Disconnected,
        };
        Disconnector {
            connection_state: connection_state,
            state: state,
        }
    }
    fn disconnect(&self) {
        if let Some(ref state) = *(self.connection_state.borrow()) {
            state.disconnect(::capnp::Error::disconnected(
                "client requested disconnect".to_owned(),
            ));
        }
    }
}

impl<VatId> Future for Disconnector<VatId>
where
    VatId: 'static,
{
    type Output = Result<(), capnp::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.state = match self.state {
            DisconnectorState::Connected => {
                self.disconnect();
                DisconnectorState::Disconnecting
            }
            DisconnectorState::Disconnecting => {
                if let Some(_) = *(self.connection_state.borrow()) {
                    DisconnectorState::Disconnecting
                } else {
                    DisconnectorState::Disconnected
                }
            }
            DisconnectorState::Disconnected => DisconnectorState::Disconnected,
        };
        match self.state {
            DisconnectorState::Connected => unreachable!(),
            DisconnectorState::Disconnecting => {
                cx.waker().clone().wake();
                Poll::Pending
            }
            DisconnectorState::Disconnected => Poll::Ready(Ok(())),
        }
    }
}

struct ResponseState<VatId>
where
    VatId: 'static,
{
    _connection_state: Rc<ConnectionState<VatId>>,
    message: Box<dyn crate::IncomingMessage>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
    _question_ref: Rc<RefCell<QuestionRef<VatId>>>,
}

enum ResponseVariant<VatId>
where
    VatId: 'static,
{
    Rpc(ResponseState<VatId>),
    LocallyRedirected(Box<dyn ResultsDoneHook>),
}

struct Response<VatId>
where
    VatId: 'static,
{
    variant: Rc<ResponseVariant<VatId>>,
}

impl<VatId> Response<VatId> {
    fn new(
        connection_state: Rc<ConnectionState<VatId>>,
        question_ref: Rc<RefCell<QuestionRef<VatId>>>,
        message: Box<dyn crate::IncomingMessage>,
        cap_table_array: Vec<Option<Box<dyn ClientHook>>>,
    ) -> Response<VatId> {
        Response {
            variant: Rc::new(ResponseVariant::Rpc(ResponseState {
                _connection_state: connection_state,
                message: message,
                cap_table: cap_table_array,
                _question_ref: question_ref,
            })),
        }
    }
    fn redirected(results_done: Box<dyn ResultsDoneHook>) -> Response<VatId> {
        Response {
            variant: Rc::new(ResponseVariant::LocallyRedirected(results_done)),
        }
    }
}

impl<VatId> Clone for Response<VatId> {
    fn clone(&self) -> Response<VatId> {
        Response {
            variant: self.variant.clone(),
        }
    }
}

impl<VatId> ResponseHook for Response<VatId> {
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>> {
        match *self.variant {
            ResponseVariant::Rpc(ref state) => {
                match state
                    .message
                    .get_body()?
                    .get_as::<message::Reader>()?
                    .which()?
                {
                    message::Return(Ok(ret)) => match ret.which()? {
                        return_::Results(Ok(mut payload)) => {
                            use ::capnp::traits::Imbue;
                            payload.imbue(&state.cap_table);
                            Ok(payload.get_content())
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            }
            ResponseVariant::LocallyRedirected(ref results_done) => results_done.get(),
        }
    }
}

struct Request<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    target: Client<VatId>,
    message: Box<dyn crate::OutgoingMessage>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
}

fn get_call<'a>(
    message: &'a mut Box<dyn crate::OutgoingMessage>,
) -> ::capnp::Result<call::Builder<'a>> {
    let message_root: message::Builder = message.get_body()?.get_as()?;
    match message_root.which()? {
        message::Call(call) => call,
        _ => unimplemented!(),
    }
}

impl<VatId> Request<VatId>
where
    VatId: 'static,
{
    fn new(
        connection_state: Rc<ConnectionState<VatId>>,
        _size_hint: Option<::capnp::MessageSize>,
        target: Client<VatId>,
    ) -> ::capnp::Result<Request<VatId>> {
        let message = connection_state.new_outgoing_message(100)?;
        Ok(Request {
            connection_state: connection_state,
            target: target,
            message: message,
            cap_table: Vec::new(),
        })
    }

    fn init_call<'a>(&'a mut self) -> call::Builder<'a> {
        let message_root: message::Builder = self.message.get_body().unwrap().get_as().unwrap();
        message_root.init_call()
    }

    fn send_internal(
        connection_state: Rc<ConnectionState<VatId>>,
        mut message: Box<dyn crate::OutgoingMessage>,
        cap_table: Vec<Option<Box<dyn ClientHook>>>,
        is_tail_call: bool,
    ) -> (
        Rc<RefCell<QuestionRef<VatId>>>,
        Promise<Response<VatId>, Error>,
    ) {
        // Build the cap table.
        let exports = ConnectionState::write_descriptors(
            &connection_state,
            &cap_table,
            get_call(&mut message).unwrap().get_params().unwrap(),
        );

        // Init the question table.  Do this after writing descriptors to avoid interference.
        let mut question = Question::<VatId>::new();
        question.is_awaiting_return = true;
        question.param_exports = exports;
        question.is_tail_call = is_tail_call;

        let question_id = connection_state.questions.borrow_mut().push(question);
        {
            let mut call_builder: call::Builder = get_call(&mut message).unwrap();
            // Finish and send.
            call_builder.reborrow().set_question_id(question_id);
            if is_tail_call {
                call_builder.get_send_results_to().set_yourself(());
            }
        }
        let _ = message.send();
        // Make the result promise.
        let (fulfiller, promise) = oneshot::channel::<Promise<Response<VatId>, Error>>();
        let promise = promise.map_err(crate::canceled_to_error).and_then(|x| x);
        let question_ref = Rc::new(RefCell::new(QuestionRef::new(
            connection_state.clone(),
            question_id,
            fulfiller,
        )));

        match connection_state.questions.borrow_mut().slots[question_id as usize] {
            Some(ref mut q) => {
                q.self_ref = Some(Rc::downgrade(&question_ref));
            }
            None => unreachable!(),
        }

        let promise = promise.attach(question_ref.clone());
        let promise2 = Promise::from_future(promise);

        (question_ref, promise2)
    }
}

impl<VatId> RequestHook for Request<VatId> {
    fn get<'a>(&'a mut self) -> any_pointer::Builder<'a> {
        use ::capnp::traits::ImbueMut;
        let mut builder = get_call(&mut self.message)
            .unwrap()
            .get_params()
            .unwrap()
            .get_content();
        builder.imbue_mut(&mut self.cap_table);
        builder
    }
    fn get_brand<'a>(&self) -> usize {
        self.connection_state.get_brand()
    }
    fn send(self: Box<Self>) -> ::capnp::capability::RemotePromise<any_pointer::Owned> {
        let tmp = *self;
        let Request {
            connection_state,
            target,
            mut message,
            cap_table,
        } = tmp;
        let write_target_result = {
            let call_builder: call::Builder = get_call(&mut message).unwrap();
            target.write_target(call_builder.get_target().unwrap())
        };
        match write_target_result {
            Some(redirect) => {
                // Whoops, this capability has been redirected while we were building the request!
                // We'll have to make a new request and do a copy.  Ick.
                let mut call_builder: call::Builder = get_call(&mut message).unwrap();
                let mut replacement = redirect.new_call(
                    call_builder.reborrow().get_interface_id(),
                    call_builder.reborrow().get_method_id(),
                    None,
                );

                replacement
                    .set(
                        call_builder
                            .get_params()
                            .unwrap()
                            .get_content()
                            .into_reader(),
                    )
                    .unwrap();
                replacement.send()
            }
            None => {
                let (question_ref, promise) =
                    Request::send_internal(connection_state.clone(), message, cap_table, false);
                let forked_promise1 = promise.shared();
                let forked_promise2 = forked_promise1.clone();

                // The pipeline must get notified of resolution before the app does to maintain ordering.
                let pipeline = Pipeline::new(
                    connection_state,
                    question_ref.clone(),
                    Some(Promise::from_future(forked_promise1)),
                );

                let resolved = pipeline.when_resolved();

                let forked_promise2 = resolved.map(|_| Ok(())).and_then(|()| forked_promise2);

                let app_promise = Promise::from_future(
                    forked_promise2
                        .map_ok(|response| ::capnp::capability::Response::new(Box::new(response))),
                );

                ::capnp::capability::RemotePromise {
                    promise: app_promise,
                    pipeline: any_pointer::Pipeline::new(Box::new(pipeline)),
                }
            }
        }
    }
    fn tail_send(self: Box<Self>) -> Option<(u32, Promise<(), Error>, Box<dyn PipelineHook>)> {
        let tmp = *self;
        let Request {
            connection_state,
            target,
            mut message,
            cap_table,
        } = tmp;

        if connection_state.connection.borrow().is_err() {
            // Disconnected; fall back to a regular send() which will fail appropriately.
            return None;
        }

        let write_target_result = {
            let call_builder: crate::rpc_capnp::call::Builder = get_call(&mut message).unwrap();
            target.write_target(call_builder.get_target().unwrap())
        };

        let (question_ref, promise) = match write_target_result {
            Some(_redirect) => {
                return None;
            }
            None => Request::send_internal(connection_state.clone(), message, cap_table, true),
        };

        let promise = promise.map_ok(|_response| {
            // Response should be null if `Return` handling code is correct.

            unimplemented!()
        });

        let question_id = question_ref.borrow().id;
        let pipeline = Pipeline::never_done(connection_state, question_ref);

        Some((
            question_id,
            Promise::from_future(promise),
            Box::new(pipeline),
        ))
    }
}

enum PipelineVariant<VatId>
where
    VatId: 'static,
{
    Waiting(Rc<RefCell<QuestionRef<VatId>>>),
    Resolved(Response<VatId>),
    Broken(Error),
}

struct PipelineState<VatId>
where
    VatId: 'static,
{
    variant: PipelineVariant<VatId>,
    redirect_later:
        Option<RefCell<futures_util::future::Shared<Promise<Response<VatId>, ::capnp::Error>>>>,
    connection_state: Rc<ConnectionState<VatId>>,

    resolve_self_promise: Promise<(), Error>,
    promise_clients_to_resolve: RefCell<
        crate::sender_queue::SenderQueue<
            (Weak<RefCell<PromiseClient<VatId>>>, Vec<PipelineOp>),
            (),
        >,
    >,
    resolution_waiters: crate::sender_queue::SenderQueue<(), ()>,
}

impl<VatId> PipelineState<VatId>
where
    VatId: 'static,
{
    fn resolve(
        state: &Rc<RefCell<PipelineState<VatId>>>,
        response: Result<Response<VatId>, Error>,
    ) {
        let to_resolve = {
            let tmp = state.borrow();
            let r = tmp.promise_clients_to_resolve.borrow_mut().drain();
            r
        };
        for ((c, ops), _) in to_resolve {
            let resolved = match response.clone() {
                Ok(v) => match v.get() {
                    Ok(x) => x.get_pipelined_cap(&ops),
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            if let Some(c) = c.upgrade() {
                c.borrow_mut().resolve(resolved);
            }
        }

        let new_variant = match response {
            Ok(r) => PipelineVariant::Resolved(r),
            Err(e) => PipelineVariant::Broken(e),
        };
        let _old_variant = mem::replace(&mut state.borrow_mut().variant, new_variant);

        let waiters = state.borrow_mut().resolution_waiters.drain();
        for (_, waiter) in waiters {
            let _ = waiter.send(());
        }
    }
}

struct Pipeline<VatId>
where
    VatId: 'static,
{
    state: Rc<RefCell<PipelineState<VatId>>>,
}

impl<VatId> Pipeline<VatId> {
    fn new(
        connection_state: Rc<ConnectionState<VatId>>,
        question_ref: Rc<RefCell<QuestionRef<VatId>>>,
        redirect_later: Option<Promise<Response<VatId>, ::capnp::Error>>,
    ) -> Pipeline<VatId> {
        let state = Rc::new(RefCell::new(PipelineState {
            variant: PipelineVariant::Waiting(question_ref),
            connection_state: connection_state.clone(),
            redirect_later: None,
            resolve_self_promise: Promise::from_future(future::pending()),
            promise_clients_to_resolve: RefCell::new(crate::sender_queue::SenderQueue::new()),
            resolution_waiters: crate::sender_queue::SenderQueue::new(),
        }));
        match redirect_later {
            Some(redirect_later_promise) => {
                let fork = redirect_later_promise.shared();
                let this = Rc::downgrade(&state);
                let resolve_self_promise =
                    connection_state.eagerly_evaluate(fork.clone().then(move |response| {
                        let state = match this.upgrade() {
                            Some(s) => s,
                            None => {
                                return Promise::err(Error::failed(
                                    "dangling reference to this".into(),
                                ))
                            }
                        };
                        PipelineState::resolve(&state, response);
                        Promise::ok(())
                    }));

                state.borrow_mut().resolve_self_promise = resolve_self_promise;
                state.borrow_mut().redirect_later = Some(RefCell::new(fork.clone()));
            }
            None => {}
        }
        Pipeline { state: state }
    }

    fn when_resolved(&self) -> Promise<(), Error> {
        self.state.borrow_mut().resolution_waiters.push(())
    }

    fn never_done(
        connection_state: Rc<ConnectionState<VatId>>,
        question_ref: Rc<RefCell<QuestionRef<VatId>>>,
    ) -> Pipeline<VatId> {
        let state = Rc::new(RefCell::new(PipelineState {
            variant: PipelineVariant::Waiting(question_ref),
            connection_state: connection_state,
            redirect_later: None,
            resolve_self_promise: Promise::from_future(future::pending()),
            promise_clients_to_resolve: RefCell::new(crate::sender_queue::SenderQueue::new()),
            resolution_waiters: crate::sender_queue::SenderQueue::new(),
        }));

        Pipeline { state: state }
    }
}

impl<VatId> PipelineHook for Pipeline<VatId> {
    fn add_ref(&self) -> Box<dyn PipelineHook> {
        Box::new(Pipeline {
            state: self.state.clone(),
        })
    }
    fn get_pipelined_cap(&self, ops: &[PipelineOp]) -> Box<dyn ClientHook> {
        self.get_pipelined_cap_move(ops.into())
    }
    fn get_pipelined_cap_move(&self, ops: Vec<PipelineOp>) -> Box<dyn ClientHook> {
        match *self.state.borrow() {
            PipelineState {
                variant: PipelineVariant::Waiting(ref question_ref),
                ref connection_state,
                ref redirect_later,
                ref promise_clients_to_resolve,
                ..
            } => {
                // Wrap a PipelineClient in a PromiseClient.
                let pipeline_client =
                    PipelineClient::new(connection_state, question_ref.clone(), ops.clone());

                match *redirect_later {
                    Some(ref _r) => {
                        let client: Client<VatId> = pipeline_client.into();
                        let promise_client =
                            PromiseClient::new(connection_state, Box::new(client), None);
                        promise_clients_to_resolve
                            .borrow_mut()
                            .push_detach((Rc::downgrade(&promise_client), ops));
                        let result: Client<VatId> = promise_client.into();
                        Box::new(result)
                    }
                    None => {
                        // Oh, this pipeline will never get redirected, so just return the PipelineClient.
                        let client: Client<VatId> = pipeline_client.into();
                        Box::new(client)
                    }
                }
            }
            PipelineState {
                variant: PipelineVariant::Resolved(ref response),
                ..
            } => response.get().unwrap().get_pipelined_cap(&ops[..]).unwrap(),
            PipelineState {
                variant: PipelineVariant::Broken(ref e),
                ..
            } => broken::new_cap(e.clone()),
        }
    }
}

pub struct Params {
    request: Box<dyn crate::IncomingMessage>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
}

impl Params {
    fn new(
        request: Box<dyn crate::IncomingMessage>,
        cap_table: Vec<Option<Box<dyn ClientHook>>>,
    ) -> Params {
        Params {
            request: request,
            cap_table: cap_table,
        }
    }
}

impl ParamsHook for Params {
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>> {
        let root: message::Reader = self.request.get_body()?.get_as()?;
        match root.which()? {
            message::Call(call) => {
                use ::capnp::traits::Imbue;
                let mut content = call?.get_params()?.get_content();
                content.imbue(&self.cap_table);
                Ok(content)
            }
            _ => unreachable!(),
        }
    }
}

enum ResultsVariant {
    Rpc(
        Box<dyn crate::OutgoingMessage>,
        Vec<Option<Box<dyn ClientHook>>>,
    ),
    LocallyRedirected(
        ::capnp::message::Builder<::capnp::message::HeapAllocator>,
        Vec<Option<Box<dyn ClientHook>>>,
    ),
}

struct ResultsInner<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    variant: Option<ResultsVariant>,
    redirect_results: bool,
    answer_id: AnswerId,
    finish_received: Rc<Cell<bool>>,
}

impl<VatId> ResultsInner<VatId>
where
    VatId: 'static,
{
    fn ensure_initialized(&mut self) {
        let answer_id = self.answer_id;
        if self.variant.is_none() {
            match (
                self.redirect_results,
                self.connection_state.connection.borrow_mut().as_mut(),
            ) {
                (false, Ok(c)) => {
                    let mut message = c.new_outgoing_message(100); // size hint?

                    {
                        let root: message::Builder = message.get_body().unwrap().init_as();
                        let mut ret = root.init_return();
                        ret.set_answer_id(answer_id);
                        ret.set_release_param_caps(false);
                    }
                    self.variant = Some(ResultsVariant::Rpc(message, Vec::new()));
                }
                _ => {
                    self.variant = Some(ResultsVariant::LocallyRedirected(
                        ::capnp::message::Builder::new_default(),
                        Vec::new(),
                    ));
                }
            }
        }
    }
}

// This takes the place of both RpcCallContext and RpcServerResponse in capnproto-c++.
pub struct Results<VatId>
where
    VatId: 'static,
{
    inner: Option<ResultsInner<VatId>>,
    results_done_fulfiller: Option<oneshot::Sender<ResultsInner<VatId>>>,
}

impl<VatId> Results<VatId>
where
    VatId: 'static,
{
    fn new(
        connection_state: &Rc<ConnectionState<VatId>>,
        answer_id: AnswerId,
        redirect_results: bool,
        fulfiller: oneshot::Sender<ResultsInner<VatId>>,
        finish_received: Rc<Cell<bool>>,
    ) -> Results<VatId> {
        Results {
            inner: Some(ResultsInner {
                variant: None,
                connection_state: connection_state.clone(),
                redirect_results: redirect_results,
                answer_id: answer_id,
                finish_received: finish_received,
            }),
            results_done_fulfiller: Some(fulfiller),
        }
    }
}

impl<VatId> Drop for Results<VatId> {
    fn drop(&mut self) {
        match (self.inner.take(), self.results_done_fulfiller.take()) {
            (Some(inner), Some(fulfiller)) => {
                let _ = fulfiller.send(inner);
            }
            (None, None) => (),
            _ => unreachable!(),
        }
    }
}

impl<VatId> ResultsHook for Results<VatId> {
    fn get<'a>(&'a mut self) -> ::capnp::Result<any_pointer::Builder<'a>> {
        use ::capnp::traits::ImbueMut;
        if let Some(ref mut inner) = self.inner {
            inner.ensure_initialized();
            match inner.variant {
                None => unreachable!(),
                Some(ResultsVariant::Rpc(ref mut message, ref mut cap_table)) => {
                    let root: message::Builder = message.get_body()?.get_as()?;
                    match root.which()? {
                        message::Return(ret) => match ret?.which()? {
                            return_::Results(payload) => {
                                let mut content = payload?.get_content();
                                content.imbue_mut(cap_table);
                                Ok(content)
                            }
                            _ => unreachable!(),
                        },
                        _ => unreachable!(),
                    }
                }
                Some(ResultsVariant::LocallyRedirected(ref mut message, ref mut cap_table)) => {
                    let mut result: any_pointer::Builder = message.get_root()?;
                    result.imbue_mut(cap_table);
                    Ok(result)
                }
            }
        } else {
            unreachable!()
        }
    }

    fn tail_call(self: Box<Self>, _request: Box<dyn RequestHook>) -> Promise<(), Error> {
        unimplemented!()
    }

    fn direct_tail_call(
        mut self: Box<Self>,
        request: Box<dyn RequestHook>,
    ) -> (Promise<(), Error>, Box<dyn PipelineHook>) {
        if let (Some(inner), Some(fulfiller)) =
            (self.inner.take(), self.results_done_fulfiller.take())
        {
            let state = inner.connection_state.clone();
            if request.get_brand() == state.get_brand() && !inner.redirect_results {
                // The tail call is headed towards the peer that called us in the first place, so we can
                // optimize out the return trip.
                if let Some((question_id, promise, pipeline)) = request.tail_send() {
                    let mut message = state.new_outgoing_message(100).expect("no connection?"); // size hint?

                    {
                        let root: message::Builder = message.get_body().unwrap().init_as();
                        let mut ret = root.init_return();
                        ret.set_answer_id(inner.answer_id);
                        ret.set_release_param_caps(false);
                        ret.set_take_from_other_question(question_id);
                    }
                    let _ = message.send();

                    // TODO cleanupanswertable

                    let _ = fulfiller.send(inner); // ??
                    return (promise, pipeline);
                }
                unimplemented!()
            } else {
                unimplemented!()
            }
        } else {
            unreachable!();
        }
    }

    fn allow_cancellation(&self) {
        unimplemented!()
    }
}

enum ResultsDoneVariant {
    Rpc(
        Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>,
        Vec<Option<Box<dyn ClientHook>>>,
    ),
    LocallyRedirected(
        ::capnp::message::Builder<::capnp::message::HeapAllocator>,
        Vec<Option<Box<dyn ClientHook>>>,
    ),
}

struct ResultsDone {
    inner: Rc<ResultsDoneVariant>,
}

impl ResultsDone {
    fn from_results_inner<VatId>(
        results_inner: Result<ResultsInner<VatId>, Error>,
        call_status: Result<(), Error>,
        pipeline_sender: queued::PipelineInnerSender,
    ) -> Result<Box<dyn ResultsDoneHook>, Error>
    where
        VatId: 'static,
    {
        match results_inner {
            Err(e) => {
                pipeline_sender.complete(Box::new(crate::broken::Pipeline::new(e.clone())));
                Err(e)
            }
            Ok(mut results_inner) => {
                results_inner.ensure_initialized();
                let ResultsInner {
                    connection_state,
                    variant,
                    answer_id,
                    finish_received,
                    ..
                } = results_inner;
                match variant {
                    None => unreachable!(),
                    Some(ResultsVariant::Rpc(mut message, cap_table)) => {
                        match (finish_received.get(), call_status) {
                            (true, _) => {
                                let hook =
                                    Box::new(ResultsDone::rpc(Rc::new(message.take()), cap_table))
                                        as Box<dyn ResultsDoneHook>;
                                pipeline_sender
                                    .complete(Box::new(local::Pipeline::new(hook.clone())));

                                // Send a Canceled return.
                                match connection_state.connection.borrow_mut().as_mut() {
                                    Ok(ref mut connection) => {
                                        let mut message = connection.new_outgoing_message(50); // XXX size hint
                                        {
                                            let root: message::Builder =
                                                message.get_body()?.get_as()?;
                                            let mut ret = root.init_return();
                                            ret.set_answer_id(answer_id);
                                            ret.set_release_param_caps(false);
                                            ret.set_canceled(());
                                        }
                                        let _ = message.send();
                                    }
                                    Err(_) => (),
                                }

                                connection_state.answer_has_sent_return(answer_id, Vec::new());
                                Ok(hook)
                            }
                            (false, Ok(())) => {
                                let exports = {
                                    let root: message::Builder = message.get_body()?.get_as()?;
                                    match root.which()? {
                                        message::Return(ret) => match ret?.which()? {
                                            crate::rpc_capnp::return_::Results(Ok(payload)) => {
                                                ConnectionState::write_descriptors(
                                                    &connection_state,
                                                    &cap_table,
                                                    payload,
                                                )
                                            }
                                            _ => unreachable!(),
                                        },
                                        _ => unreachable!(),
                                    }
                                };

                                let (_promise, m) = message.send();
                                connection_state.answer_has_sent_return(answer_id, exports);
                                let hook = Box::new(ResultsDone::rpc(m, cap_table))
                                    as Box<dyn ResultsDoneHook>;
                                pipeline_sender
                                    .complete(Box::new(local::Pipeline::new(hook.clone())));
                                Ok(hook)
                            }
                            (false, Err(e)) => {
                                // Send an error return.
                                match connection_state.connection.borrow_mut().as_mut() {
                                    Ok(ref mut connection) => {
                                        let mut message = connection.new_outgoing_message(50); // XXX size hint
                                        {
                                            let root: message::Builder =
                                                message.get_body()?.get_as()?;
                                            let mut ret = root.init_return();
                                            ret.set_answer_id(answer_id);
                                            ret.set_release_param_caps(false);
                                            let mut exc = ret.init_exception();
                                            from_error(&e, exc.reborrow());
                                        }
                                        let _ = message.send();
                                    }
                                    Err(_) => (),
                                }
                                connection_state.answer_has_sent_return(answer_id, Vec::new());

                                pipeline_sender
                                    .complete(Box::new(crate::broken::Pipeline::new(e.clone())));

                                Err(e)
                            }
                        }
                    }
                    Some(ResultsVariant::LocallyRedirected(results_done, cap_table)) => {
                        let hook = Box::new(ResultsDone::redirected(results_done, cap_table))
                            as Box<dyn ResultsDoneHook>;
                        pipeline_sender
                            .complete(Box::new(crate::local::Pipeline::new(hook.clone())));
                        Ok(hook)
                    }
                }
            }
        }
    }

    fn rpc(
        message: Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>,
        cap_table: Vec<Option<Box<dyn ClientHook>>>,
    ) -> ResultsDone {
        ResultsDone {
            inner: Rc::new(ResultsDoneVariant::Rpc(message, cap_table)),
        }
    }

    fn redirected(
        message: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
        cap_table: Vec<Option<Box<dyn ClientHook>>>,
    ) -> ResultsDone {
        ResultsDone {
            inner: Rc::new(ResultsDoneVariant::LocallyRedirected(message, cap_table)),
        }
    }
}

impl ResultsDoneHook for ResultsDone {
    fn add_ref(&self) -> Box<dyn ResultsDoneHook> {
        Box::new(ResultsDone {
            inner: self.inner.clone(),
        })
    }
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>> {
        use ::capnp::traits::Imbue;
        match *self.inner {
            ResultsDoneVariant::Rpc(ref message, ref cap_table) => {
                let root: message::Reader = message.get_root_as_reader()?;
                match root.which()? {
                    message::Return(ret) => match ret?.which()? {
                        crate::rpc_capnp::return_::Results(payload) => {
                            let mut content = payload?.get_content();
                            content.imbue(cap_table);
                            Ok(content)
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            }
            ResultsDoneVariant::LocallyRedirected(ref message, ref cap_table) => {
                let mut result: any_pointer::Reader = message.get_root_as_reader()?;
                result.imbue(cap_table);
                Ok(result)
            }
        }
    }
}

enum ClientVariant<VatId>
where
    VatId: 'static,
{
    Import(Rc<RefCell<ImportClient<VatId>>>),
    Pipeline(Rc<RefCell<PipelineClient<VatId>>>),
    Promise(Rc<RefCell<PromiseClient<VatId>>>),
    __NoIntercept(()),
}

struct Client<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    variant: ClientVariant<VatId>,
}

enum WeakClientVariant<VatId>
where
    VatId: 'static,
{
    Import(Weak<RefCell<ImportClient<VatId>>>),
    Pipeline(Weak<RefCell<PipelineClient<VatId>>>),
    Promise(Weak<RefCell<PromiseClient<VatId>>>),
    __NoIntercept(()),
}

struct WeakClient<VatId>
where
    VatId: 'static,
{
    connection_state: Weak<ConnectionState<VatId>>,
    variant: WeakClientVariant<VatId>,
}

impl<VatId> WeakClient<VatId>
where
    VatId: 'static,
{
    fn upgrade(&self) -> Option<Client<VatId>> {
        let variant = match self.variant {
            WeakClientVariant::Import(ref ic) => match ic.upgrade() {
                Some(ic) => ClientVariant::Import(ic),
                None => return None,
            },
            WeakClientVariant::Pipeline(ref pc) => match pc.upgrade() {
                Some(pc) => ClientVariant::Pipeline(pc),
                None => return None,
            },
            WeakClientVariant::Promise(ref pc) => match pc.upgrade() {
                Some(pc) => ClientVariant::Promise(pc),
                None => return None,
            },
            WeakClientVariant::__NoIntercept(()) => ClientVariant::__NoIntercept(()),
        };
        let state = match self.connection_state.upgrade() {
            Some(s) => s,
            None => return None,
        };
        Some(Client {
            connection_state: state,
            variant: variant,
        })
    }
}

struct ImportClient<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    import_id: ImportId,

    /// Number of times we've received this import from the peer.
    remote_ref_count: u32,
}

impl<VatId> Drop for ImportClient<VatId> {
    fn drop(&mut self) {
        let connection_state = self.connection_state.clone();

        assert!(connection_state
            .client_downcast_map
            .borrow_mut()
            .remove(&((self) as *const _ as usize))
            .is_some());

        // Remove self from the import table, if the table is still pointing at us.
        let mut remove = false;
        if let Some(import) = connection_state.imports.borrow().slots.get(&self.import_id) {
            if let Some((_, ptr)) = import.import_client {
                if ptr == ((&*self) as *const _ as usize) {
                    remove = true;
                }
            }
        }

        if remove {
            connection_state
                .imports
                .borrow_mut()
                .slots
                .remove(&self.import_id);
        }

        // Send a message releasing our remote references.
        let mut tmp = connection_state.connection.borrow_mut();
        match (self.remote_ref_count > 0, tmp.as_mut()) {
            (true, Ok(ref mut c)) => {
                let mut message = c.new_outgoing_message(50); // XXX size hint
                {
                    let root: message::Builder = message.get_body().unwrap().init_as();
                    let mut release = root.init_release();
                    release.set_id(self.import_id);
                    release.set_reference_count(self.remote_ref_count);
                }
                let _ = message.send();
            }
            _ => (),
        }
    }
}

impl<VatId> ImportClient<VatId>
where
    VatId: 'static,
{
    fn new(
        connection_state: &Rc<ConnectionState<VatId>>,
        import_id: ImportId,
    ) -> Rc<RefCell<ImportClient<VatId>>> {
        Rc::new(RefCell::new(ImportClient {
            connection_state: connection_state.clone(),
            import_id: import_id,
            remote_ref_count: 0,
        }))
    }

    fn add_remote_ref(&mut self) {
        self.remote_ref_count += 1;
    }
}

impl<VatId> From<Rc<RefCell<ImportClient<VatId>>>> for Client<VatId> {
    fn from(client: Rc<RefCell<ImportClient<VatId>>>) -> Client<VatId> {
        let connection_state = client.borrow().connection_state.clone();
        Client::new(&connection_state, ClientVariant::Import(client))
    }
}

/// A `ClientHook` representing a pipelined promise.  Always wrapped in `PromiseClient`.
struct PipelineClient<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    question_ref: Rc<RefCell<QuestionRef<VatId>>>,
    ops: Vec<PipelineOp>,
}

impl<VatId> PipelineClient<VatId>
where
    VatId: 'static,
{
    fn new(
        connection_state: &Rc<ConnectionState<VatId>>,
        question_ref: Rc<RefCell<QuestionRef<VatId>>>,
        ops: Vec<PipelineOp>,
    ) -> Rc<RefCell<PipelineClient<VatId>>> {
        Rc::new(RefCell::new(PipelineClient {
            connection_state: connection_state.clone(),
            question_ref: question_ref,
            ops: ops,
        }))
    }
}

impl<VatId> From<Rc<RefCell<PipelineClient<VatId>>>> for Client<VatId> {
    fn from(client: Rc<RefCell<PipelineClient<VatId>>>) -> Client<VatId> {
        let connection_state = client.borrow().connection_state.clone();
        Client::new(&connection_state, ClientVariant::Pipeline(client))
    }
}

impl<VatId> Drop for PipelineClient<VatId> {
    fn drop(&mut self) {
        assert!(self
            .connection_state
            .client_downcast_map
            .borrow_mut()
            .remove(&((self) as *const _ as usize))
            .is_some());
    }
}

/// A `ClientHook` that initially wraps one client and then, later on, redirects
/// to some other client.
struct PromiseClient<VatId>
where
    VatId: 'static,
{
    connection_state: Rc<ConnectionState<VatId>>,
    is_resolved: bool,
    cap: Box<dyn ClientHook>,
    import_id: Option<ImportId>,
    received_call: bool,
    resolution_waiters: crate::sender_queue::SenderQueue<(), Box<dyn ClientHook>>,
}

impl<VatId> PromiseClient<VatId> {
    fn new(
        connection_state: &Rc<ConnectionState<VatId>>,
        initial: Box<dyn ClientHook>,
        import_id: Option<ImportId>,
    ) -> Rc<RefCell<PromiseClient<VatId>>> {
        Rc::new(RefCell::new(PromiseClient {
            connection_state: connection_state.clone(),
            is_resolved: false,
            cap: initial,
            import_id: import_id,
            received_call: false,
            resolution_waiters: crate::sender_queue::SenderQueue::new(),
        }))
    }

    fn resolve(&mut self, replacement: Result<Box<dyn ClientHook>, Error>) {
        let (mut replacement, is_error) = match replacement {
            Ok(v) => (v, false),
            Err(e) => (broken::new_cap(e), true),
        };
        let connection_state = self.connection_state.clone();
        let is_connected = connection_state.connection.borrow().is_ok();
        let replacement_brand = replacement.get_brand();
        if replacement_brand != connection_state.get_brand()
            && self.received_call
            && !is_error
            && is_connected
        {
            // The new capability is hosted locally, not on the remote machine.  And, we had made calls
            // to the promise.  We need to make sure those calls echo back to us before we allow new
            // calls to go directly to the local capability, so we need to set a local embargo and send
            // a `Disembargo` to echo through the peer.
            let (fulfiller, promise) = oneshot::channel::<Result<(), Error>>();
            let promise = promise
                .map_err(crate::canceled_to_error)
                .and_then(|v| future::ready(v));
            let embargo = Embargo::new(fulfiller);
            let embargo_id = connection_state.embargoes.borrow_mut().push(embargo);

            let mut message = connection_state
                .new_outgoing_message(50)
                .expect("no connection?"); // XXX size hint
            {
                let root: message::Builder = message.get_body().unwrap().init_as();
                let mut disembargo = root.init_disembargo();
                disembargo
                    .reborrow()
                    .init_context()
                    .set_sender_loopback(embargo_id);
                let target = disembargo.init_target();

                let redirect = connection_state.write_target(&*self.cap, target);
                if redirect.is_some() {
                    panic!("Original promise target should always be from this RPC connection.")
                }
            }

            // Make a promise which resolves to `replacement` as soon as the `Disembargo` comes back.
            let embargo_promise = promise.map_ok(move |()| replacement);

            let mut queued_client = queued::Client::new(None);
            let weak_queued = Rc::downgrade(&queued_client.inner);

            queued_client.drive(embargo_promise.then(move |r| {
                if let Some(q) = weak_queued.upgrade() {
                    queued::ClientInner::resolve(&q, r);
                }
                Promise::ok(())
            }));

            // We need to queue up calls in the meantime, so we'll resolve ourselves to a local promise
            // client instead.
            replacement = Box::new(queued_client);

            let _ = message.send();
        }

        for ((), waiter) in self.resolution_waiters.drain() {
            let _ = waiter.send(replacement.clone());
        }

        let old_cap = mem::replace(&mut self.cap, replacement);
        connection_state.add_task(async move {
            drop(old_cap);
            Ok(())
        });

        self.is_resolved = true;
    }
}

impl<VatId> Drop for PromiseClient<VatId> {
    fn drop(&mut self) {
        let self_ptr = (self) as *const _ as usize;

        if let Some(id) = self.import_id {
            // This object is representing an import promise.  That means the import table may still
            // contain a pointer back to it.  Remove that pointer.  Note that we have to verify that
            // the import still exists and the pointer still points back to this object because this
            // object may actually outlive the import.
            let ref mut slots = self.connection_state.imports.borrow_mut().slots;
            if let Some(ref mut import) = slots.get_mut(&id) {
                let mut drop_it = false;
                if let Some(ref c) = import.app_client {
                    if let Some(cs) = c.upgrade() {
                        if cs.get_ptr() == self_ptr {
                            drop_it = true;
                        }
                    }
                }
                if drop_it {
                    import.app_client = None;
                }
            }
        }

        assert!(self
            .connection_state
            .client_downcast_map
            .borrow_mut()
            .remove(&self_ptr)
            .is_some());
    }
}

impl<VatId> From<Rc<RefCell<PromiseClient<VatId>>>> for Client<VatId> {
    fn from(client: Rc<RefCell<PromiseClient<VatId>>>) -> Client<VatId> {
        let connection_state = client.borrow().connection_state.clone();
        Client::new(&connection_state, ClientVariant::Promise(client))
    }
}

impl<VatId> Client<VatId> {
    fn new(
        connection_state: &Rc<ConnectionState<VatId>>,
        variant: ClientVariant<VatId>,
    ) -> Client<VatId> {
        let client = Client {
            connection_state: connection_state.clone(),
            variant: variant,
        };
        let weak = client.downgrade();

        // XXX arguably, this should go in each of the variant's constructors.
        connection_state
            .client_downcast_map
            .borrow_mut()
            .insert(client.get_ptr(), weak);
        client
    }
    fn downgrade(&self) -> WeakClient<VatId> {
        let variant = match self.variant {
            ClientVariant::Import(ref import_client) => {
                WeakClientVariant::Import(Rc::downgrade(import_client))
            }
            ClientVariant::Pipeline(ref pipeline_client) => {
                WeakClientVariant::Pipeline(Rc::downgrade(pipeline_client))
            }
            ClientVariant::Promise(ref promise_client) => {
                WeakClientVariant::Promise(Rc::downgrade(promise_client))
            }
            _ => unimplemented!(),
        };
        WeakClient {
            connection_state: Rc::downgrade(&self.connection_state),
            variant: variant,
        }
    }

    fn from_ptr(ptr: usize, connection_state: &ConnectionState<VatId>) -> Option<Client<VatId>> {
        match connection_state.client_downcast_map.borrow().get(&ptr) {
            Some(c) => c.upgrade(),
            None => None,
        }
    }

    fn write_target(
        &self,
        mut target: crate::rpc_capnp::message_target::Builder,
    ) -> Option<Box<dyn ClientHook>> {
        match self.variant {
            ClientVariant::Import(ref import_client) => {
                target.set_imported_cap(import_client.borrow().import_id);
                None
            }
            ClientVariant::Pipeline(ref pipeline_client) => {
                let mut builder = target.init_promised_answer();
                let question_ref = &pipeline_client.borrow().question_ref;
                builder.set_question_id(question_ref.borrow().id);
                let mut transform =
                    builder.init_transform(pipeline_client.borrow().ops.len() as u32);
                for idx in 0..pipeline_client.borrow().ops.len() {
                    match pipeline_client.borrow().ops[idx] {
                        ::capnp::private::capability::PipelineOp::GetPointerField(ordinal) => {
                            transform
                                .reborrow()
                                .get(idx as u32)
                                .set_get_pointer_field(ordinal);
                        }
                        _ => {}
                    }
                }
                None
            }
            ClientVariant::Promise(ref promise_client) => {
                promise_client.borrow_mut().received_call = true;
                self.connection_state
                    .write_target(&*promise_client.borrow().cap, target)
            }
            _ => unimplemented!(),
        }
    }

    fn write_descriptor(&self, mut descriptor: cap_descriptor::Builder) -> Option<u32> {
        match self.variant {
            ClientVariant::Import(ref import_client) => {
                descriptor.set_receiver_hosted(import_client.borrow().import_id);
                None
            }
            ClientVariant::Pipeline(ref pipeline_client) => {
                let mut promised_answer = descriptor.init_receiver_answer();
                let question_ref = &pipeline_client.borrow().question_ref;
                promised_answer.set_question_id(question_ref.borrow().id);
                let mut transform =
                    promised_answer.init_transform(pipeline_client.borrow().ops.len() as u32);
                for idx in 0..pipeline_client.borrow().ops.len() {
                    match pipeline_client.borrow().ops[idx] {
                        ::capnp::private::capability::PipelineOp::GetPointerField(ordinal) => {
                            transform
                                .reborrow()
                                .get(idx as u32)
                                .set_get_pointer_field(ordinal);
                        }
                        _ => {}
                    }
                }

                None
            }
            ClientVariant::Promise(ref promise_client) => {
                promise_client.borrow_mut().received_call = true;

                ConnectionState::write_descriptor(
                    &self.connection_state.clone(),
                    &promise_client.borrow().cap.clone(),
                    descriptor,
                )
                .unwrap()
            }
            _ => unimplemented!(),
        }
    }
}

impl<VatId> Clone for Client<VatId> {
    fn clone(&self) -> Client<VatId> {
        let variant = match self.variant {
            ClientVariant::Import(ref import_client) => {
                ClientVariant::Import(import_client.clone())
            }
            ClientVariant::Pipeline(ref pipeline_client) => {
                ClientVariant::Pipeline(pipeline_client.clone())
            }
            ClientVariant::Promise(ref promise_client) => {
                ClientVariant::Promise(promise_client.clone())
            }
            _ => unimplemented!(),
        };
        Client {
            connection_state: self.connection_state.clone(),
            variant: variant,
        }
    }
}

impl<VatId> ClientHook for Client<VatId> {
    fn add_ref(&self) -> Box<dyn ClientHook> {
        Box::new(self.clone())
    }
    fn new_call(
        &self,
        interface_id: u64,
        method_id: u16,
        size_hint: Option<::capnp::MessageSize>,
    ) -> ::capnp::capability::Request<any_pointer::Owned, any_pointer::Owned> {
        let request: Box<dyn RequestHook> =
            match Request::new(self.connection_state.clone(), size_hint, self.clone()) {
                Ok(mut request) => {
                    {
                        let mut call_builder = request.init_call();
                        call_builder.set_interface_id(interface_id);
                        call_builder.set_method_id(method_id);
                    }
                    Box::new(request)
                }
                Err(e) => Box::new(broken::Request::new(e, None)),
            };

        ::capnp::capability::Request::new(request)
    }

    fn call(
        &self,
        interface_id: u64,
        method_id: u16,
        params: Box<dyn ParamsHook>,
        mut results: Box<dyn ResultsHook>,
    ) -> Promise<(), Error> {
        // Implement call() by copying params and results messages.

        let maybe_request = params.get().and_then(|p| {
            let mut request = p
                .target_size()
                .and_then(|s| Ok(self.new_call(interface_id, method_id, Some(s))))?;
            request.get().set_as(p)?;
            Ok(request)
        });

        match maybe_request {
            Err(e) => Promise::err(e.clone()),
            Ok(request) => {
                let ::capnp::capability::RemotePromise { promise, .. } = request.send();

                let promise = promise.and_then(move |response| {
                    pry!(pry!(results.get()).set_as(pry!(response.get())));
                    Promise::ok(())
                });

                Promise::from_future(promise)
            }
        }
        // TODO implement this in terms of direct tail call.
        // We can and should propagate cancellation.
        // (TODO ?)
        // context -> allowCancellation();

        //results.direct_tail_call(request.hook)
    }

    fn get_ptr(&self) -> usize {
        match self.variant {
            ClientVariant::Import(ref import_client) => {
                (&*import_client.borrow()) as *const _ as usize
            }
            ClientVariant::Pipeline(ref pipeline_client) => {
                (&*pipeline_client.borrow()) as *const _ as usize
            }
            ClientVariant::Promise(ref promise_client) => {
                (&*promise_client.borrow()) as *const _ as usize
            }
            _ => unimplemented!(),
        }
    }

    fn get_brand(&self) -> usize {
        self.connection_state.get_brand()
    }

    fn get_resolved(&self) -> Option<Box<dyn ClientHook>> {
        match self.variant {
            ClientVariant::Import(ref _import_client) => None,
            ClientVariant::Pipeline(ref _pipeline_client) => None,
            ClientVariant::Promise(ref promise_client) => {
                if promise_client.borrow().is_resolved {
                    Some(promise_client.borrow().cap.clone())
                } else {
                    None
                }
            }
            _ => unimplemented!(),
        }
    }

    fn when_more_resolved(&self) -> Option<Promise<Box<dyn ClientHook>, Error>> {
        match self.variant {
            ClientVariant::Import(ref _import_client) => None,
            ClientVariant::Pipeline(ref _pipeline_client) => None,
            ClientVariant::Promise(ref promise_client) => {
                Some(promise_client.borrow_mut().resolution_waiters.push(()))
            }
            _ => unimplemented!(),
        }
    }

    fn when_resolved(&self) -> Promise<(), Error> {
        default_when_resolved_impl(self)
    }
}

pub(crate) fn default_when_resolved_impl<C>(client: &C) -> Promise<(), Error>
where
    C: ClientHook,
{
    match client.when_more_resolved() {
        Some(promise) => {
            Promise::from_future(promise.and_then(|resolution| resolution.when_resolved()))
        }
        None => Promise::ok(()),
    }
}

// ===================================

struct SingleCapPipeline {
    cap: Box<dyn ClientHook>,
}

impl SingleCapPipeline {
    fn new(cap: Box<dyn ClientHook>) -> SingleCapPipeline {
        SingleCapPipeline { cap: cap }
    }
}

impl PipelineHook for SingleCapPipeline {
    fn add_ref(&self) -> Box<dyn PipelineHook> {
        Box::new(SingleCapPipeline {
            cap: self.cap.clone(),
        })
    }
    fn get_pipelined_cap(&self, ops: &[PipelineOp]) -> Box<dyn ClientHook> {
        if ops.is_empty() {
            self.cap.add_ref()
        } else {
            broken::new_cap(Error::failed("Invalid pipeline transform.".to_string()))
        }
    }
}
