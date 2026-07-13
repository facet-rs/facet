+++
title = "rodin"
+++

Rodin is the version solver: given manifests and a package index, it picks
one version per package so every requirement is satisfied. It is written in
vix and runs as an ordinary demanded computation — resolution is a value,
solved when something asks for it.

The [solver specification](/spec) is the single normative surface. Its
fixture corpus uses Cargo as the executable oracle for Cargo-domain behavior.
