extern crate capnpc;

fn main() {
    ::capnpc::CompilerCommand::new()
        .file("http.capnp")
        .run()
        .unwrap();
}
