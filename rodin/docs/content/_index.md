+++
title = "rodin"
sort_by = "weight"
+++

Rodin is the version solver: given manifests and a package index, it picks
one version per package so every requirement is satisfied. It is written in
vix and runs as an ordinary demanded computation — resolution is a value,
solved when something asks for it.

These pages are its normative specification. The working design corpus
(oracle, fixtures, identity, constraints, search, features, targets) lives
alongside the source in `rodin/docs/`.
