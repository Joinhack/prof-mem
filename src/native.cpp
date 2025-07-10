#include <thread>

thread_local int ALLOC_ENTRY = 0;


extern "C" {
    int alloc_entry_increase() {
        ALLOC_ENTRY += 1;
        return ALLOC_ENTRY;
    }

    int alloc_entry_decrease() {
        ALLOC_ENTRY -= 1;
        return ALLOC_ENTRY;
    }
}
