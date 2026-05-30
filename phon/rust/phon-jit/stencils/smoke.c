// Spine smoke stencil: proves the clang -> object-extract -> MAP_JIT exec path.
//
// Pure arithmetic, no external references, so clang emits a self-contained run
// of instructions with no relocations. We extract its bytes at build time and
// execute them from JIT memory at run time. clang chooses every instruction; we
// encode nothing.

long phon_stencil_smoke(long x) {
    return x * 3 + 1;
}
