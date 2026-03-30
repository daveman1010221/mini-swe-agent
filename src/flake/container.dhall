-- src/flake/container.dhall
--
-- mini-swe-agent dev container configuration.
-- This file is the single place where project-specific container decisions live.
-- All the how is in nix-container-lib. This file is pure what.
--
-- Library reference: github:daveman1010221/nix-container-lib
-- Type reference:    dhall/types.dhall
-- Defaults:          dhall/defaults.dhall

let Lib      = PRELUDE_PATH
let defaults = Lib.defaults

let FailureMode = Lib.FailureMode
let Input       = Lib.StageInput
let Output      = Lib.StageOutput

-- PipelineOutputArtifact and PipelineOutputAssertion are not re-exported
-- from prelude.dhall, so we spell out the None type inline.
let NoPipelineOutputs =
      None { artifacts : List { name             : Text
                               , fromStage        : Text
                               , artifact         : Text
                               , attestation      : Optional Text
                               , verifyMethod     : Optional Text
                               }
           , assertions : List { name : Text, fromStage : Text }
           }

-- ---------------------------------------------------------------------------
-- Pipeline
--
-- fmt and clippy run unconditionally and collect all findings before
-- reporting. Static analysis also collects. The test stage is gated on
-- CI_FULL so developers don't pay for it on every save, but CI always
-- sets that var.
-- ---------------------------------------------------------------------------
let msweaPipeline : Lib.PipelineConfig =
  { name        = "mswea-ci"
  , artifactDir = "/workspace/pipeline-out"
  , workingDir  = "/workspace"
  , outputs     = NoPipelineOutputs
  , stages      =
      [ { name           = "fmt"
        , command        = "cargo fmt --check --all"
        , failureMode    = FailureMode.Collect
        , inputs         = [ Input.Workspace ]
        , outputs        = [ Output.Assertion { name = "formatted", description = Some "Source passes rustfmt" } ]
        , condition      = None Text
        , pure           = True
        , impurityReason = None Text
        }
      , { name           = "clippy"
        , command        = "cargo clippy --workspace -- -D warnings"
        , failureMode    = FailureMode.Collect
        , inputs         = [ Input.Workspace, Input.Toolchain ]
        , outputs        = [ Output.Assertion { name = "lint-clean", description = Some "No clippy warnings" } ]
        , condition      = None Text
        , pure           = True
        , impurityReason = None Text
        }
      , { name           = "static-analysis"
        , command        = "run-analysis --config ./analysis.toml"
        , failureMode    = FailureMode.Collect
        , inputs         = [ Input.Workspace ]
        , outputs        = [ Output.Report { name = Some "static-analysis-report" } ]
        , condition      = None Text
        , pure           = True
        , impurityReason = None Text
        }
      , { name           = "test"
        , command        = "cargo test --workspace"
        , failureMode    = FailureMode.FailFast
        , inputs         = [ Input.Workspace, Input.Toolchain ]
        , outputs        = [ Output.Assertion { name = "tests-pass", description = Some "All workspace tests pass" } ]
        , condition      = Some "CI_FULL"
        , pure           = False
        , impurityReason = Some "Cannot guarantee CI_FULL is set"
        }
      ]
  }

-- ---------------------------------------------------------------------------
-- Extra packages from flake inputs.
-- staticanalysis and dotacat come from named flake inputs.
-- nvim-pkg lands in pkgs via myNeovimOverlay so Lib.nixpkgs resolves it.
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
--
-- Derived from defaults.devContainer with mswea-specific overrides:
-- - No TLS (no service layer requiring mTLS)
-- - SSH disabled, port 2223 reserved (matches run-dev-container in Justfile)
-- - Pipeline defined above
-- ---------------------------------------------------------------------------
in defaults.devContainer //
  { name = "mswea-dev"

  , shell = Some
      { shell       = "/bin/nu"
      , colorScheme = "gruvbox"
      , viBindings  = True
      , plugins     = [] : List Text
      }

  , packageLayers =
      [ Lib.PackageLayer.Core
      , Lib.PackageLayer.CI
      , Lib.PackageLayer.Dev
      , Lib.PackageLayer.Toolchain
      , Lib.PackageLayer.Pipeline
      , msweaExtras
      ]

  , pipeline = Some msweaPipeline

  , tls = None Lib.TLSConfig

  , ssh = Some (defaults.defaultSSH // { enable = True, port = 2223 })

  , extraEnv =
      [ Lib.buildEnv "RUST_LOG"          "info"
      , Lib.buildEnv "CARGO_HTTP_CAINFO" "/etc/ssl/certs/ca-bundle.crt"
      ]
  }
