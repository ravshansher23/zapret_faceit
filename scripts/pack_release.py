"""Собирает zip только с файлами, которые нужны для запуска на Windows."""

from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "dist" / "faceit-bypass.zip"
FOLDER = "faceit-bypass"

FILES = [
    "faceit-bypass.exe",
    "Запуск.bat",
    "Запуск ALT.bat",
    "Стоп.bat",
    "README.md",
    "LICENSE",
    "lists/list-faceit.txt",
    "lists/ipset-exclude.txt",
]


def main() -> None:
    missing = [name for name in FILES if not (ROOT / name).is_file()]
    bin_dir = ROOT / "bin"
    bin_files = sorted(path for path in bin_dir.iterdir() if path.is_file()) if bin_dir.is_dir() else []
    if not (bin_dir / "winws.exe").is_file():
        missing.append("bin/winws.exe")
    if missing:
        raise SystemExit("нет файлов для архива:\n" + "\n".join(missing))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(OUT, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in FILES:
            write(archive, ROOT / name, f"{FOLDER}/{name}")
        for path in bin_files:
            write(archive, path, f"{FOLDER}/bin/{path.name}")
    print(OUT)


def write(archive: zipfile.ZipFile, path: Path, arcname: str) -> None:
    info = zipfile.ZipInfo.from_file(path, arcname)
    info.flag_bits |= 0x800
    with path.open("rb") as source:
        archive.writestr(info, source.read(), compress_type=zipfile.ZIP_DEFLATED)


if __name__ == "__main__":
    main()
