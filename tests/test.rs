use std::thread;

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
    let join = thread::spawn(|| {
        for i in 0..1000 {
            if i == 100 {
                dump().unwrap();
            }
        }
    });
    join.join().unwrap();
}
