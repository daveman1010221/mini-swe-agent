{
  ai = null;
  entrypoint = null;
  extraEnv = [
    { name = "RUST_LOG"; placement = u: u.BuildTime; value = "info"; }
    {
      name = "CARGO_HTTP_CAINFO";
      placement = u:
        u.BuildTime;
      value = "/etc/ssl/certs/ca-bundle.crt";
    }
  ];
  mode = u:
    u.Dev;
  name = "mswea-dev";
  nix = {
    buildUserCount = u:
      u.Dynamic;
    enableDaemon = true;
    sandboxPolicy = u:
      u.Auto;
    trustedUsers = [ "root" ];
  };
  packageLayers = [
    (u:
      u.Core)
    (u:
      u.CI)
    (u:
      u.Dev)
    (u:
      u.Toolchain)
    (u:
      u.Pipeline)
    (u:
      u.Custom {
        name = "mswea-extras";
        packages = [
          { attrPath = "default"; flakeInput = "staticanalysis"; }
          { attrPath = "default"; flakeInput = "dotacat"; }
          { attrPath = "default"; flakeInput = "myNeovimOverlay"; }
          { attrPath = "just"; flakeInput = null; }
          { attrPath = "jq"; flakeInput = null; }
          { attrPath = "dropbear"; flakeInput = null; }
        ];
      })
  ];
  pipeline = {
    artifactDir = "/workspace/pipeline-out";
    name = "mswea-ci";
    outputs = null;
    stages = [
      {
        command = "cargo fmt --check --all";
        condition = null;
        failureMode = u:
          u.Collect;
        impurityReason = null;
        inputs = [ (u: u.Workspace) ];
        name = "fmt";
        outputs = [
          (u:
            u.Assertion {
              description = "Source passes rustfmt";
              name = "formatted";
            })
        ];
        pure = true;
      }
      {
        command = "cargo clippy --workspace -- -D warnings";
        condition = null;
        failureMode = u:
          u.Collect;
        impurityReason = null;
        inputs = [ (u: u.Workspace) (u: u.Toolchain) ];
        name = "clippy";
        outputs = [
          (u:
            u.Assertion {
              description = "No clippy warnings";
              name = "lint-clean";
            })
        ];
        pure = true;
      }
      {
        command = "run-analysis --config ./analysis.toml";
        condition = null;
        failureMode = u:
          u.Collect;
        impurityReason = null;
        inputs = [ (u: u.Workspace) ];
        name = "static-analysis";
        outputs = [ (u: u.Report { name = "static-analysis-report"; }) ];
        pure = true;
      }
      {
        command = "cargo test --workspace";
        condition = "CI_FULL";
        failureMode = u:
          u.FailFast;
        impurityReason = "Cannot guarantee CI_FULL is set";
        inputs = [ (u: u.Workspace) (u: u.Toolchain) ];
        name = "test";
        outputs = [
          (u:
            u.Assertion {
              description = "All workspace tests pass";
              name = "tests-pass";
            })
        ];
        pure = false;
      }
    ];
    workingDir = "/workspace";
  };
  shell = {
    colorScheme = "gruvbox";
    plugins = [];
    shell = "/bin/nu";
    viBindings = true;
  };
  ssh = { enable = true; port = 2223; };
  staticGid = null;
  staticUid = null;
  tls = null;
  user = {
    createUser = true;
    defaultShell = "/bin/fish";
    skeletonPath = "/etc/container-skel";
    supplementalGroups = [];
  };
}
