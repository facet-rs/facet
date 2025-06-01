{
  description = "facet devel";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  # shell.nix compatibility
  inputs.flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    ...
  }: let
    # System types to support.
    targetSystems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];

    # Helper function to generate an attrset '{ x86_64-linux = f "x86_64-linux"; ... }'.
    forAllSystems = nixpkgs.lib.genAttrs targetSystems;

    pkgsFor = forAllSystems (system:
      import nixpkgs {
        localSystem.system = system;
        overlays = [rust-overlay.overlays.default];
      });
  in {
    devShells =
      nixpkgs.lib.mapAttrs (
        system: pkgs: {
          default = pkgs.mkShell {
            strictDeps = true;
            packages = with pkgs; [
              (rust-bin.stable.latest.default.override {
                extensions = ["rust-analyzer"];
              })
              cargo-nextest
            ];
          };
        }
      )
      pkgsFor;

    formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.alejandra);
  };
}
