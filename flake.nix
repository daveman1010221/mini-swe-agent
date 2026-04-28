{
  description = "mini-swe-agent — a Rust port with embedded nushell and ractor-based actors";

  inputs = {
    nixpkgs.url          = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url      = "github:numtide/flake-utils";
    rust-overlay.url     = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    crane.url            = "github:ipetkov/crane";

    myNeovimOverlay.url                        = "github:daveman1010221/nix-neovim";
    myNeovimOverlay.inputs.nixpkgs.follows     = "nixpkgs";
    myNeovimOverlay.inputs.flake-utils.follows = "flake-utils";

    staticanalysis.url                         = "github:daveman1010221/polar-static-analysis";
    staticanalysis.inputs.nixpkgs.follows      = "nixpkgs";
    staticanalysis.inputs.flake-utils.follows  = "flake-utils";
    staticanalysis.inputs.rust-overlay.follows = "rust-overlay";

    dotacat.url                    = "github:daveman1010221/dotacat-fast";
    dotacat.inputs.nixpkgs.follows = "nixpkgs";

    nix-container-lib.url                        = "github:daveman1010221/nix-container-lib";
    nix-container-lib.inputs.nixpkgs.follows     = "nixpkgs";
    nix-container-lib.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane,
              myNeovimOverlay, staticanalysis, dotacat, nix-container-lib, ... } @ inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
          overlays = [
            rust-overlay.overlays.default
            myNeovimOverlay.overlays.default
          ];
        };
        inherit (pkgs) lib;

        # ── Dev container ──────────────────────────────────────────────────────
        # src/flake/container.nix is the pre-rendered output of
        # src/flake/container.dhall. Regenerate it with:
        #   just render-container
        #
        # inputs is still passed through so mkContainer can resolve flake
        # packages (staticanalysis, dotacat, myNeovimOverlay) referenced
        # by name in the rendered Nix attrset.
        container = nix-container-lib.lib.${system}.mkContainer {
          inherit system pkgs inputs;
          configNixPath = ./src/flake/container.nix;
        };

        # ── Rust toolchain ─────────────────────────────────────────────────────
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "clippy" "rustfmt" "rust-analyzer" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config clang ];
          buildInputs = with pkgs; [ openssl ]
            ++ lib.optionals stdenv.isDarwin [ libiconv ];
          CARGO_BUILD_RUSTFLAGS = "-C linker=clang -C link-arg=-fuse-ld=lld";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        msweaPackages = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

      in {
        packages = {
          default      = msweaPackages;
          devContainer = container.image;
        };

        checks = {
          mswea-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });

          mswea-fmt = craneLib.cargoFmt {
            src = craneLib.cleanCargoSource ./.;
          };

          mswea-test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ container.devShell ];
          shellHook = ''
            export OPENAI_BASE_URL="http://172.26.26.88:8080/v1"
            export OPENAI_API_KEY="sk-local"
          '';
        };
      }
    );
}
