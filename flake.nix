{
  description = "mitchty/flakelight-rust basic example";
  inputs = {
    flakelight.url = "github:nix-community/flakelight";
    flakelight-rust.url = "github:mitchty/flakelight-rust";
  };
  outputs =
    {
      self,
      flakelight,
      flakelight-rust,
      ...
    }:
    flakelight ./. (
      { lib, ... }:
      {
        imports = [ flakelight-rust.flakelightModules.default ];
        inputs.self = self;

        systems = [
          "x86_64-linux"
          "aarch64-linux"
          "aarch64-darwin"
        ];

        # Provide a loosey goose deny.toml setup.
        fileset = lib.fileset.unions [
          (lib.fileset.fileFilter (f: f.hasExt "rs" || f.name == "Cargo.toml") ./.)
          (./. + /Cargo.lock)
          ./deny.toml
        ];

        defaultBinary = "perdido";

        binaries.perdido = {
          variants = {
            default = { };
            portable = {
              portable = true;
            };
            windows = {
              target = "x86_64-pc-windows-gnu";

              # So that we specify this only builds from here. Bit hacky might
              # fix it later.
              systems = [
                "x86_64-linux"
                "aarch64-linux"
              ];
            };
          };
        };
      }
    );
}
