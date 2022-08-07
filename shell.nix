{ pkgs ? import <nixpkgs>
  {
    overlays = [ (import <rust-overlay>) ];
  }
}:
let
  pkg_rust = pkgs.rust-bin.stable.latest.default.override {
    targets = [
      "x86_64-unknown-linux-gnu"
      "x86_64-pc-windows-gnu"
      "arm-unknown-linux-gnueabihf"
    ];
  };
in
  pkgs.mkShell {
    nativeBuildInputs = [
      pkg_rust
      pkgs.gnumake
      pkgs.pkg-config
      pkgs.gmock
      pkgs.glxinfo
      pkgs.vulkan-tools
      pkgs.xorg.libX11
      pkgs.xorg.libXrandr
      pkgs.libudev
    ];
    LD_LIBRARY_PATH = with pkgs.xlibs; "${pkgs.vulkan-loader}/lib:${pkgs.mesa}/lib:${libX11}/lib:${libXcursor}/lib:${libXxf86vm}/lib:${libXi}/lib:${libXrandr}/lib";
    DISPLAY = ":0";
  }
