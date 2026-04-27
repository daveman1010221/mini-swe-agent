-- src/flake/container.dhall
--
-- mini-swe-agent dev container configuration.
-- This file is the single place where project-specific container decisions live.
-- All the how is in nix-container-lib. This file is pure what.
--
-- Library reference: github:daveman1010221/nix-container-lib
-- Type reference:    dhall/types.dhall
-- Defaults:          dhall/defaults.dhall

let Lib = https://raw.githubusercontent.com/daveman1010221/nix-container-lib/e2334448bd4bb6348a467244d474f907d3d0e36d/dhall/prelude.dhall
let defaults = Lib.defaults

-- ---------------------------------------------------------------------------
-- Extra packages from flake inputs.
-- ---------------------------------------------------------------------------
let msweaExtras =
  Lib.customLayer "mswea-extras"
    [ Lib.flakePackage "staticanalysis"   "default"
    , Lib.flakePackage "dotacat"          "default"
    , Lib.flakePackage "myNeovimOverlay"  "default"
    , Lib.nixpkgs "just"
    , Lib.nixpkgs "jq"
    , Lib.nixpkgs "dropbear"
    ]

-- ---------------------------------------------------------------------------
-- Container configuration.
-- ---------------------------------------------------------------------------
in defaults.devContainer //
  { name = "mswea-dev"

  , shell = Some (Lib.Shell.Interactive
      { shell       = "/bin/nu"
      , colorScheme = "gruvbox"
      , viBindings  = True
      , plugins     = [] : List Text
      })

  , packageLayers =
      [ Lib.PackageLayer.Micro
      , Lib.PackageLayer.Core
      , Lib.PackageLayer.InteractiveDev
      , Lib.PackageLayer.RustToolchain
      , msweaExtras
      ]

  , pipeline = None Lib.PipelineConfig

  , tls = None Lib.TLSConfig

  , ssh = Some (defaults.defaultSSH // { enable = True, port = 2223 })

  , extraEnv =
      [ Lib.buildEnv "RUST_LOG"          "info"
      , Lib.buildEnv "CARGO_HTTP_CAINFO" "/etc/ssl/certs/ca-bundle.crt"
      ]
  }
