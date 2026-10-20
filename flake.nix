{
  description = "capstone-optics-number-cruncher project flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
    }:
    let
      system = "x86_64-linux";

      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = [
          "rust-src"
          "rustfmt"
          "clippy"
        ];
      };

      craneLib = (crane.mkLib pkgs).overrideToolchain rust;

      # Tools needed at build time
      nativeBuildInputs = with pkgs; [
        makeWrapper
        pkg-config
        python3
      ];

      # Libraries needed at build and runtime
      buildInputs = with pkgs; [
        dbus
        libxcb
        libXcursor
        libXrandr
        libXi
        libX11
        wayland
        libxkbcommon
        libGL
        libGLU
      ];

      # Common arguments for both the dependency build and the final package build
      commonArgs = {
        inherit nativeBuildInputs buildInputs;

        pname = "capstone-optics-number-cruncher";
        version = "0.1.0";
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;
      };

      # 1. Build just the cargo dependencies (cached)
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      # 2. Build the actual project, reusing the cached dependencies
      default-pkg = craneLib.buildPackage (
        commonArgs
        // {
          inherit cargoArtifacts;

          postInstall = ''
            wrapProgram $out/bin/capstone-optics-number-cruncher \
              --prefix LD_LIBRARY_PATH : "/run/opengl-driver/lib:${
                pkgs.lib.makeLibraryPath [
                  pkgs.libGL
                  pkgs.libGLU
                  pkgs.libxkbcommon
                ]
              }"
          '';
        }
      );

    in
    {
      # `nix build` will produce the binary
      packages.${system}.default = default-pkg;

      # `nix develop` will drop you into the shell
      devShells.${system}.default = pkgs.mkShell {
        inherit nativeBuildInputs;

        # The shell needs everything the build needs, plus the rust toolchain itself
        buildInputs = buildInputs ++ [
          rust
        ];

        shellHook = ''
          export LD_LIBRARY_PATH=/run/opengl-driver/lib/:${
            pkgs.lib.makeLibraryPath [
              pkgs.libGL
              pkgs.libGLU
              pkgs.libxkbcommon
            ]
          }

          GREEN="\[\033[0;32m\]"
          RESET="\[\033[0m\]"
          export PS1="$GREEN (DSP-dev)$RESET $PS1"
        '';
      };
    };
}
