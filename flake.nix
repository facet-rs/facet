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
  in {
    devShells =
      lib.mapAttrs (system: pkgs: {
        default = pkgs.mkShell {
          strictDeps = true;
          packages = with pkgs;
            [
              # Must occur first to take precedence over nightly.
              # The windows-msvc target lets us cross-compile the nextest
              # archive on this Linux host for the QEMU Windows VM (see the
              # `test-windows` Justfile recipe / scripts/winvm).
              (rust-bin.stable.latest.minimal.override {
                extensions = ["rust-src" "rust-docs" "clippy"];
                targets = ["x86_64-pc-windows-msvc"];
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
            # Windows-in-QEMU test harness: MSVC cross-compile toolchain plus
            # the VM plumbing. Linux-only — none of this is needed (or builds)
            # on the darwin dev shells.
            ++ lib.optionals stdenv.isLinux [
              cargo-xwin # drives clang-cl/lld-link + downloads the MSVC SDK
              llvmPackages.clang-unwrapped # provides clang-cl (no nix cc-wrapper flags)
              llvmPackages.lld # provides lld-link
              llvmPackages.llvm # provides llvm-lib / llvm-ar for cc-built deps

              qemu
              swtpm # emulated TPM 2.0 (Win11 install requirement)
              xorriso # build the autounattend secondary CD
              mtools
              aria2 # resumable ISO download
              openssh
              curl
              jq
            ];

          RUST_BACKTRACE = 1;
          RUST_LOG = "debug";

          # OVMF UEFI firmware for the Windows VM (Win11 needs UEFI + TPM).
          # Consumed by scripts/winvm/run-qemu.sh.
          OVMF_FD = lib.optionalString pkgs.stdenv.isLinux "${pkgs.OVMF.fd}/FV";
        };
      })
      pkgsFor;

    formatter = eachSystem (system: nixpkgs.legacyPackages.${system}.alejandra);
  };
}
