use prof_mem::{ProfAlloc, dump};

#[global_allocator]
static ALLOC: ProfAlloc = ProfAlloc(128);

#[test]
fn test_print() {
    for i in 0..1000 {
        if i == 100 {
            dump().unwrap();
        }
    }
}
