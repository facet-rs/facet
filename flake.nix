{
  description = "facet devel";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    ...
  }: let
    inherit (nixpkgs) lib;

    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];

    eachSystem = lib.genAttrs systems;
    pkgsFor = eachSystem (system:
      import nixpkgs {
        localSystem.system = system;
        overlays = [rust-overlay.overlays.default];
      });
  in {
    devShells =
      lib.mapAttrs (system: pkgs: {
        default = pkgs.mkShell {
          strictDeps = true;
          packages = with pkgs; [
            # Must occur first to take precedence over nightly.
            (rust-bin.stable.latest.minimal.override {
              extensions = ["rust-src" "rust-docs" "clippy"];
            })

            # Use `rustfmt`, and other tools that require nightly features.
            (rust-bin.selectLatestNightlyWith (toolchain:
              toolchain.minimal.override {
                extensions = ["rustfmt" "rust-analyzer"];
              }))

            cargo-nextest
            just
          ];

          RUST_BACKTRACE = 1;
          RUST_LOG = "debug";
        };
      })
      pkgsFor;

    formatter = eachSystem (system: nixpkgs.legacyPackages.${system}.alejandra);
  };
}
