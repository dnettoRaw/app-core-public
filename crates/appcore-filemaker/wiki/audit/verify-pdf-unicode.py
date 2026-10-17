"""Check fixed Unicode PDF samples with Poppler; run the pdf_unicode example first.

The unmarked RTL sample accepts logical or Poppler raw visual order. Latin
scalars are compared exactly, without Unicode normalization. Review PNGs too.
"""
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path

CRATE = Path(__file__).resolve().parents[2]
WORKSPACE = CRATE.parents[1]
PDFS = WORKSPACE / "target/filemaker-examples"
OUT = WORKSPACE / "target/appcore/filemaker-pdf-unicode"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(mode, source):
    pdf = PDFS / f"pdf-unicode-{mode}.pdf"
    info = subprocess.check_output(["pdfinfo", str(pdf)], text=True)
    pages = re.search(r"^Pages:\s+(\d+)$", info, re.MULTILINE)
    require(pages is not None and int(pages[1]) == 1, "unexpected Unicode sample page count")
    subprocess.run(["pdftotext", "-raw", str(pdf), str(OUT / f"{mode}.txt")], check=True)
    subprocess.run(["pdftoppm", "-f", "1", "-l", "1", "-singlefile", "-r", "110", "-png",
                    str(pdf), str(OUT / mode)], check=True)
    text = (OUT / f"{mode}.txt").read_text()
    if mode == "flattened":
        require(not text.strip(), "Flattened unexpectedly has extracted text")
    else:
        lines = text.splitlines()
        for line in source["latin"].splitlines():
            require(line in lines, f"{mode}: incomplete or normalized Latin line")
        require(source["title"] in lines, "missing title")
        require(lines.count(source["isolated"]) == 1, "isolated multi-glyph cluster was lost or duplicated")
        controls = dict.fromkeys(map(ord, "\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069"))
        rtl = [line.translate(controls) for line in lines]
        require(source["arabic"] in rtl or source["arabic"][::-1] in rtl,
                "RTL sample has missing or duplicated characters")
    return dict(mode=mode, pages=1, sha256=hashlib.sha256(pdf.read_bytes()).hexdigest(),
                exact_latin_scalars=mode != "flattened", outline_only=mode == "flattened")


def main():
    for command in ["pdfinfo", "pdftoppm", "pdftotext"]:
        require(shutil.which(command) is not None, f"required QA tool missing: {command}")
    OUT.mkdir(parents=True, exist_ok=True)
    scene = json.loads((PDFS / "pdf-unicode-scene.json").read_text())
    source = {element["id"]: element["text"] for element in scene["pages"][0]["elements"]}
    cases = [verify(mode, source) for mode in ["editable", "hybrid", "flattened"]]
    hybrid = hashlib.sha256((OUT / "hybrid.png").read_bytes()).hexdigest()
    flattened = hashlib.sha256((OUT / "flattened.png").read_bytes()).hexdigest()
    require(hybrid == flattened, "Hybrid text layer changed visible outline pixels")
    result = dict(cases=cases, hybrid_flattened_identical_pixels=True,
                  isolated_multi_glyph_cluster_verified=True, normalization_applied=False)
    (OUT / "extraction.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Unicode samples verified; Hybrid/Flattened pixels are identical")


if __name__ == "__main__":
    main()
