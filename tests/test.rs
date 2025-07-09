use prof_mem::{ProfAlloc, dump};

#[global_allocator]
static ALLOC: ProfAlloc = ProfAlloc(128);

#[test]
fn test_print() {
    println!("aaa");
    dump();
}
