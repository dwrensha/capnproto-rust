extern crate capnpc;

fn main() {
    ::capnpc::CompilerCommand::new()
        .file("calculator.capnp")
        .run()
        .unwrap();
}
