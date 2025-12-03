+++
title = "Pull Request Process"
weight = 2
insert_anchor_links = "heading"
+++

1. **Create a branch** — never commit directly to `main`

2. **Write tests** — ensure your changes are covered

3. **Run checks locally**:
   ```bash
   just           # Full test suite
   just miri      # Memory safety
   just nostd-ci  # no_std compatibility
   ```

4. **Push and open a PR** with `gh pr create`

5. **CI must pass** — the test matrix includes:
   - Tests (Linux, macOS, Windows)
   - no_std build
   - Miri
   - MSRV check
   - Clippy
   - Documentation build

## Generated Files

Do **not** edit `README.md` files directly. Edit `README.md.in` instead — READMEs are generated.
