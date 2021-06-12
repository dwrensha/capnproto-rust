fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("tests/")
        .file("tests/capnp/test.capnp")
        .run()
        .unwrap();
}
