
#include <stdint.h>

#if defined(_WIN32) || defined(_WIN64)
    #define EXPORT __declspec(dllexport)
#elif defined(__GNUC__)
    #define EXPORT __attribute__((visibility("default")))
#else
    #define EXPORT
#endif

typedef struct Bar {
    int32_t a;
    int32_t b;
} Bar;

typedef struct Foo {
    int64_t x;
    Bar bar;
    uint32_t y;
} Foo;

EXPORT const char* get_library_message(void) {
    return "IAMA C lib AMA";
}

EXPORT Foo* get_foo(void) {
    static Foo sample_foo = {
        .x = 42,
        .bar = {
            .a = 10,
            .b = 20
        },
        .y = 30
    };

    return &sample_foo;
}
