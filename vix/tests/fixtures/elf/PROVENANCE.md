# ELF fixture provenance

Generated on `miles.local` (`Linux miles 7.0.0-27-generic`, x86_64,
Ubuntu glibc `2.43-2ubuntu2`) with `/usr/bin/cc`.

Commands:

```sh
cc -O2 -g0 hello.c -o hello-x86_64
cc -O2 -g0 close_range.c -o close-range-x86_64
cc -O2 -g0 -fPIC -shared tiny_shared.c -o libtiny-x86_64.so
```

Sources:

```c
/* hello.c */
#include <stdio.h>
int main(void) { puts("hello from vix elf fixture"); return 0; }

/* close_range.c */
#define _GNU_SOURCE
#include <unistd.h>
int main(void) { return close_range(3, 3, 0); }

/* tiny_shared.c */
#include <string.h>
int vix_fixture_len(const char *s) { return (int)strlen(s); }
```

SHA-256:

```text
5a5fcd2aaa3de91c8aefdd31a0c9abdc91b78678f75689c044bf3aef74e054a7  hello-x86_64
cbdbaa5a1011834e7265376c1b775af6e539bfb336deddb5a24c3b9664a6dadd  close-range-x86_64
9cf067d6404069bb998471fc6441f7cbd6b7389e26c01ec866f8bf42ebf95d38  libtiny-x86_64.so
```

No aarch64 fixture was generated in this slice: `miles.local` had no
`aarch64-linux-gnu-gcc` in PATH, and `rustc --target
aarch64-unknown-linux-gnu` reported that the target standard library was not
installed.
