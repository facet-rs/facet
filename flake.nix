{
  description = "The Facet project is a collection of crates pioneering runtime reflection in Rust.";

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

    # capn (aka captain) is the pre-commit / pre-push automation this repo runs
    # from its git hooks. It lives at github:bearcove/capn and ships prebuilt
    # cargo-dist release binaries, so we vendor the pinned binary declaratively
    # (fetch + autoPatchelf) instead of `cargo install --git`. This keeps the
    # hooks fully nix-native and, crucially, makes the glibc binaries run on
    # NixOS, where they otherwise fail to find the dynamic linker.
    #
    # To bump: change capnVersion, then update the hashes with
    #   nix store prefetch-file --json <release-url> | grep -o 'sha256-[^"]*'
    capnVersion = "1.4.0";
    capnTargets = {
      "x86_64-linux" = {
        target = "x86_64-unknown-linux-gnu";
        hash = "sha256-FUmHA9t9z9M6GsP12I+g3YEhbzLlPWgtrxd0AIO/3ak=";
      };
      "aarch64-linux" = {
        target = "aarch64-unknown-linux-gnu";
        hash = "sha256-mKDHfHSQ+7fcg9woeauPkugRFX8SAl0Ru+c2Cchh+no=";
      };
      # cargo-dist only publishes an aarch64 macOS build; x86_64-darwin has no
      # prebuilt asset, so capn is simply absent from that dev shell.
      "aarch64-darwin" = {
        target = "aarch64-apple-darwin";
        hash = "sha256-H+3J4c2u39x92vFGoI59+w9YrZMxriMA7lfPgJ1bM8I=";
      };
    };

    capnFor = eachSystem (
      system: let
        pkgs = pkgsFor.${system};
        meta = capnTargets.${system} or null;
      in
        if meta == null
        then null
        else
          pkgs.stdenv.mkDerivation {
            pname = "capn";
            version = capnVersion;

            src = pkgs.fetchurl {
              url = "https://github.com/bearcove/capn/releases/download/v${capnVersion}/captain-${meta.target}.tar.xz";
              inherit (meta) hash;
            };

            sourceRoot = "captain-${meta.target}";

            nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [pkgs.autoPatchelfHook];
            buildInputs = lib.optionals pkgs.stdenv.isLinux [pkgs.stdenv.cc.cc.lib];

            installPhase = ''
              runHook preInstall
              install -Dm755 captain "$out/bin/capn"
              ln -s capn "$out/bin/captain"
              runHook postInstall
            '';

            meta = {
              description = "Pre-commit, pre-push, rust-centric automation (bearcove/capn)";
              homepage = "https://github.com/bearcove/capn";
              license = with lib.licenses; [mit asl20];
              mainProgram = "capn";
              platforms = lib.attrNames capnTargets;
            };
          }
    );
  in {
    packages = lib.mapAttrs (system: capn:
      lib.optionalAttrs (capn != null) {inherit capn;})
    capnFor;

    devShells =
      lib.mapAttrs (system: pkgs: {
        default = pkgs.mkShell {
          strictDeps = true;
          packages = with pkgs;
            [
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
              cargo-insta
              just
            ]
            # Provide the git-hook runner from nix, patched to run on NixOS.
            ++ lib.optional (capnFor.${system} != null) capnFor.${system};

          RUST_BACKTRACE = 1;
          RUST_LOG = "debug";
        };
      })
      pkgsFor;

    formatter = eachSystem (system: nixpkgs.legacyPackages.${system}.alejandra);
  };
}
