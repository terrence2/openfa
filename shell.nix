{ pkgs ? import <nixpkgs>
  {
    overlays = [];
  }
}:
let
in
  pkgs.mkShell {
    nativeBuildInputs = [
      pkgs.gnumake
      pkgs.pkg-config
      pkgs.fontconfig
      pkgs.gmock
      pkgs.glxinfo
      pkgs.vulkan-tools
      pkgs.xorg.libX11
      pkgs.xorg.libXrandr
      pkgs.udev
    ];
    LD_LIBRARY_PATH = with pkgs.xorg; "${pkgs.vulkan-loader}/lib:${pkgs.mesa}/lib:${libX11}/lib:${libXcursor}/lib:${libXxf86vm}/lib:${libXi}/lib:${libXrandr}/lib";
    DISPLAY = ":0";
  }
