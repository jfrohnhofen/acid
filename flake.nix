{
  description = "ACID electronics design environment (KiCad/atopile)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forEachSupportedSystem = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };
      });
    in
    {
      devShells = forEachSupportedSystem ({ pkgs }:
        let
          # atopile requires Python 3.13, GCC 16 <cstdint> fix, env var expansion in fp-lib-table,
          # and KiCad 10 pcb version tolerance
          atopile = pkgs.python313Packages.atopile.overridePythonAttrs (old: {
            postPatch = (old.postPatch or "") + ''
              sed -i '1i#include <cstdint>' src/faebryk/core/cpp/include/util.hpp
              sed -i 's/str(lib.uri)/os.path.expandvars(str(lib.uri))/' src/faebryk/libs/part_lifecycle.py
              sed -i 's/loads(path, C_kicad_pcb_file)/loads(path, C_kicad_pcb_file, ignore_assertions=True)/' src/faebryk/libs/kicad/fileformats_version.py
              sed -i 's/KICAD_VERSION_NAMES = {/KICAD_VERSION_NAMES = {\n    20260206: "KiCad 10.0",/' src/faebryk/libs/kicad/fileformats_version.py
              sed -i 's/pcb.kicad_pcb.version != KICAD_PCB_VERSION/pcb.kicad_pcb.version not in KICAD_VERSION_NAMES/' src/faebryk/libs/kicad/fileformats_version.py
              sed -i 's/allow_missing_courtyard = auto()/allow_missing_courtyard = auto()\n        allow_soldermask_bridges = auto()/' src/faebryk/libs/kicad/fileformats_latest.py
              sed -i 's/net_name: str/net_name: Optional[str] = None/' src/faebryk/libs/kicad/fileformats_latest.py
              sed -i 's/filled_areas_thickness: bool/filled_areas_thickness: Optional[bool] = None/' src/faebryk/libs/kicad/fileformats_latest.py
              python3 -c '
path = "src/faebryk/libs/sexp/dataclass_sexp.py"
content = open(path).read()
target = "    if not hasattr(t, \"__dataclass_fields__\"):"
replacement = "    if getattr(t, \"__name__\", None) == \"C_net\" and len(sexp) == 1:\n        sexp = [0, sexp[0]]\n" + target
assert target in content
content = content.replace(target, replacement, 1)

target_convert = "        if isinstance(val, Symbol):\n            return t(str(val))\n\n        return t(val)"
replacement_convert = """        if isinstance(val, Symbol):
            val = str(val)
        if t is int and isinstance(val, str):
            try:
                return int(val)
            except ValueError:
                return val
        return t(val)"""
assert target_convert in content
content = content.replace(target_convert, replacement_convert, 1)
open(path, "w").write(content)

path2 = "src/faebryk/libs/kicad/fileformats_version.py"
content2 = open(path2).read()
target2 = "return loads(path, C_kicad_pcb_file, ignore_assertions=True)"
replacement2 = """out = loads(path, C_kicad_pcb_file, ignore_assertions=True)
        from faebryk.libs.kicad.fileformats_latest import C_net
        existing_names = {n.name for n in out.kicad_pcb.nets if n.name}
        for fp in out.kicad_pcb.footprints:
            for pad in fp.pads:
                if pad.net and pad.net.name and pad.net.name not in existing_names:
                    existing_names.add(pad.net.name)
                    out.kicad_pcb.nets.append(C_net(number=len(out.kicad_pcb.nets), name=pad.net.name))
        for track in (out.kicad_pcb.segments + out.kicad_pcb.arcs + out.kicad_pcb.vias + out.kicad_pcb.zones):
            if hasattr(track, "net") and isinstance(track.net, str) and track.net not in existing_names:
                existing_names.add(track.net)
                out.kicad_pcb.nets.append(C_net(number=len(out.kicad_pcb.nets), name=track.net))
        for z in out.kicad_pcb.zones:
            if getattr(z, "net_name", None) is None and hasattr(z, "net"):
                z.net_name = str(z.net)
        return out"""
assert target2 in content2
open(path2, "w").write(content2.replace(target2, replacement2, 1))

path3 = "src/atopile/build_steps.py"
content3 = open(path3).read()
target3 = """        pcb_original_normalized = round_dataclass(
            sort_dataclass(original_pcb_file, sort_key=str, inplace=False), precision=2
        )
        pcb_normalized = round_dataclass(
            sort_dataclass(pcb_file, sort_key=str, inplace=False), precision=2
        )

        pcb_diff = compare_dataclasses(
            before=pcb_original_normalized,
            after=pcb_normalized,
            skip_keys=("uuid", "__atopile_lib_fp_hash__"),
            require_dataclass_type_match=False,
        )"""
replacement3 = """        pcb_file_copy = deepcopy(pcb_file)
        original_pcb_file_copy = deepcopy(original_pcb_file)
        for _p in (pcb_file_copy, original_pcb_file_copy):
            for _fp in _p.kicad_pcb.footprints:
                for _pad in _fp.pads:
                    if _pad.net is not None:
                        _pad.net.number = 0
            for _net in _p.kicad_pcb.nets:
                _net.number = 0
            for _track in (_p.kicad_pcb.segments + _p.kicad_pcb.arcs + _p.kicad_pcb.vias + _p.kicad_pcb.zones):
                if hasattr(_track, "net") and isinstance(_track.net, int):
                    _track.net = 0

        pcb_original_normalized = round_dataclass(
            sort_dataclass(original_pcb_file_copy, sort_key=str, inplace=False), precision=2
        )
        pcb_normalized = round_dataclass(
            sort_dataclass(pcb_file_copy, sort_key=str, inplace=False), precision=2
        )

        pcb_diff = compare_dataclasses(
            before=pcb_original_normalized,
            after=pcb_normalized,
            skip_keys=("uuid", "__atopile_lib_fp_hash__", "number"),
            require_dataclass_type_match=False,
        )"""
assert target3 in content3
content3 = content3.replace(target3, replacement3, 1)

target4 = "        elif original_pcb_file == pcb_file:"
replacement4 = "        elif not pcb_diff:"
assert target4 in content3
content3 = content3.replace(target4, replacement4, 1)
open(path3, "w").write(content3)
'
            '';
          });
          atopileAlias = pkgs.writeShellScriptBin "atopile" ''exec "${atopile}/bin/ato" "$@"'';

          pcbShell = pkgs.mkShell {
            name = "acid-pcb-shell";

            packages = [
              atopile
              atopileAlias
              pkgs.kicad
              pkgs.git
            ];

            shellHook = ''
              export KICAD10_FOOTPRINT_DIR="${pkgs.kicad.libraries.footprints}/share/kicad/footprints"
              export KICAD10_TEMPLATE_DIR="${pkgs.kicad.libraries.templates}/share/kicad/template"
              export KICAD10_SYMBOL_DIR="${pkgs.kicad.libraries.symbols}/share/kicad/symbols"
              export KICAD10_3DMODEL_DIR="${pkgs.kicad.libraries.packages3d}/share/kicad/3dmodels"

              alias atopile=ato

              echo "⚡ ACID PCB environment loaded"
              echo "   • atopile: $(ato --version 2>/dev/null || echo 'not found') (command: ato or atopile)"
              echo "   • KiCad:   $(kicad-cli version 2>/dev/null || echo 'not found')"
            '';
          };
        in
        {
          default = pcbShell;
          pcb = pcbShell;
        });
    };
}
