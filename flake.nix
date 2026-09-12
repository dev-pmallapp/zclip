{
  description = "zclip - a Zellij plugin implementing a tmux-like yank-buffer ring";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    # The whole reason this flake needs an overlay at all: `zclip` only ever
    # builds for wasm32-wasip1, and nixpkgs' own `rustc` does not ship a std
    # for that target. rust-overlay does, and -- more importantly -- it can
    # read rust-toolchain.toml directly (see `toolchain` below), so the target
    # list stays declared in exactly one place for cargo, CI and Nix alike.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Honour rust-toolchain.toml rather than restating the channel and
        # target here. That file already declares `targets = ["wasm32-wasip1"]`
        # for rustup users; duplicating it in Nix is how the two silently drift
        # until a `nix build` succeeds against a target `cargo build` cannot
        # reach, or vice versa.
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

        # Kept in sync with workspace.package.version in Cargo.toml. The release
        # workflow already fails a tag that disagrees with Cargo.toml (see
        # docs/dev.md), so Cargo.toml stays the single source of truth and this
        # is read from it rather than typed out again.
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      in
      {
        packages.default = self.packages.${system}.zclip;

        packages.zclip = rustPlatform.buildRustPackage {
          pname = "zclip";
          inherit (cargoToml.workspace.package) version;

          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # Not the usual native build, so the stock cargoBuildHook/
          # cargoInstallHook pair is bypassed on both ends:
          #
          # - buildPhase: the hooks would build for the host triple, which is
          #   exactly what docs/dev.md warns never to do -- linking `zclip`
          #   natively drags zellij-utils' isahc -> curl -> openssl-sys chain
          #   into a build that has no use for any of it.
          # - installPhase: cargoInstallHook looks for executables to copy into
          #   $out/bin. A .wasm is not an executable in that sense, so it would
          #   install nothing at all and still exit 0.
          buildPhase = ''
            runHook preBuild
            cargo build --workspace --target wasm32-wasip1 --release --offline
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            install -Dm444 target/wasm32-wasip1/release/zclip.wasm \
              $out/bin/zclip.wasm
            # The example config ships alongside the plugin because zclip is
            # inert without it: every entry point (copy mode, paste, the
            # buffer list) is reached through a `MessagePlugin` keybinding the
            # user has to write, so a bare .wasm cannot be invoked at all.
            # Installing only the binary would hand someone a plugin with no
            # way to reach it and nothing to copy from.
            install -Dm444 examples/zclip.kdl \
              $out/share/zclip/zclip.kdl
            runHook postInstall
          '';

          # `just test` scope, restated: the test suite is zclip-core's, and
          # zclip-core is pure Rust with no host API -- but a wasm32-wasip1
          # `cargo test` produces wasm test binaries this builder cannot
          # execute. Tests belong in CI (and `just test`) on the host target,
          # not here. Building the wasm artifact is what this derivation is for.
          doCheck = false;

          # cargo-auditable rewrites the produced binary to embed a dependency
          # manifest. That is an ELF-shaped operation and has no business
          # touching a .wasm that Zellij will validate byte-for-byte.
          auditable = false;

          meta = {
            inherit (cargoToml.workspace.package) description;
            homepage = cargoToml.workspace.package.repository;
            license = pkgs.lib.licenses.mit;
            # Not a runnable program: it is a WASI module loaded by Zellij.
            # Leaving mainProgram unset keeps `nix run` from pretending
            # otherwise.
            platforms = pkgs.lib.platforms.all;
          };
        };

        # docs/dev.md points NixOS users at `nix-shell -p just cargo-watch`
        # for the dev loop and `binaryen` for `just dist`. This is that, plus
        # the pinned toolchain, so the loop needs no rustup install.
        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.just
            pkgs.cargo-watch
            pkgs.binaryen
            pkgs.zellij
          ];
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
