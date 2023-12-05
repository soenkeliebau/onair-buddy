{ pkgs ? import <nixpkgs> {}
}: pkgs.mkShellNoCC rec {
  packages = [ pkgs.clang pkgs.pipewire pkgs.pkg-config pkgs.rustc pkgs.cargo pkgs.llvm ];
  LIBCLANG_PATH = "${pkgs.llvmPackages_11.libclang.lib}/lib";
}