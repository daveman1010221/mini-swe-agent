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
      u.InteractiveDev)
    (u:
      u.RustToolchain)
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
  pipeline = null;
  shell = u:
    u.Interactive {
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
