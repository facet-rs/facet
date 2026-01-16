+++
title = "Pull Request Process"
weight = 2
insert_anchor_links = "heading"
+++

## Workflow

1. Create a branch — never commit directly to `main`
2. Make your changes
3. Run `just ci` locally
4. Push and open a PR with `gh pr create`

## Generated files

Do **not** edit `README.md` files directly. Edit `README.md.in` instead — READMEs are generated.

### captain

The [captain](https://github.com/bearcove/captain) tool keeps all crates in sync. It generates READMEs from templates and ensures consistent metadata across the monorepo.

## Licensing

Everything in the monorepo is dual-licensed under MIT and Apache 2.0. If you want your crate in the monorepo, it must use the same license.
