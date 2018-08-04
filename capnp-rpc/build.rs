extern crate capnpc;

fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("schema")
        .file("schema/rpc.capnp")
        .file("schema/rpc-twoparty.capnp")
        .run()
        .expect("capnp compile");
}
