"""Render and verify the fixed synthetic corpus; requires Poppler, no Python packages.

Run the variable_corpus Rust example first. This checks fixture output, not tax
rules, scanner certification, or the general completeness of PDF text layers.
Visual review of the emitted page PNGs remains required.
"""
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path

CRATE = Path(__file__).resolve().parents[2]
WORKSPACE = CRATE.parents[1]
OUTPUT = WORKSPACE / "target/filemaker-examples"
EVIDENCE = WORKSPACE / "target/appcore/filemaker-variable-corpus"
CASES = [(1, 3), (2, 12), (5, 40), (20, 190)]
# Existing fixed EAN-13 conformance pattern, excluding quiet zones.
PATTERN = (
    "101000110101001110101111011110100010010110011010101"
    "00001010000101000010111010010000101100110101"
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify_bars(pages, scene):
    code = next(e for e in scene["pages"][-1]["elements"] if e["id"] == "code")
    bounds = code["bounds"]["layout"]
    prefix = EVIDENCE / f"barcode-{pages}"
    subprocess.run(
        ["pdftoppm", "-f", str(pages), "-l", str(pages), "-singlefile",
         "-r", "144", "-gray", str(OUTPUT / f"variable-corpus-{pages}.pdf"), str(prefix)],
        check=True,
    )
    pgm = prefix.with_suffix(".pgm").read_bytes()
    header = re.match(rb"P5\s+(\d+)\s+(\d+)\s+255\s", pgm)
    require(header is not None, "unexpected Poppler grayscale header")
    width, height = map(int, header.groups())
    pixels = pgm[header.end():]
    require(len(pixels) == width * height, "incomplete grayscale raster")
    x, y = (bounds["origin"][axis] / 1_000_000 for axis in ["x", "y"])
    w, h = (bounds["size"][axis] / 1_000_000 for axis in ["width", "height"])

    def sample(module):
        px = int((x + (module + 0.5) * w / 113) * 2)
        py = int((y + h / 2) * 2)
        require(0 <= px < width and 0 <= py < height, "barcode outside raster")
        return pixels[py * width + px]

    bits = "".join("1" if sample(11 + i) < 128 else "0" for i in range(95))
    require(bits == PATTERN, f"page-count {pages}: incorrect rendered barcode")
    for module in [*range(11), *range(106, 113)]:
        require(sample(module) > 245, f"page-count {pages}: painted quiet zone")


def verify_case(pages, groups):
    pdf = OUTPUT / f"variable-corpus-{pages}.pdf"
    info = subprocess.check_output(["pdfinfo", str(pdf)], text=True)
    actual_pages = re.search(r"^Pages:\s+(\d+)$", info, re.MULTILINE)
    require(actual_pages is not None and int(actual_pages[1]) == pages, "page count mismatch")
    (EVIDENCE / f"pdfinfo-{pages}.txt").write_text(info)
    subprocess.run(
        ["pdftoppm", "-f", "1", "-l", str(pages), "-r", "72", "-png",
         str(pdf), str(EVIDENCE / f"pages-{pages}")], check=True,
    )
    text_file = EVIDENCE / f"text-{pages}.txt"
    subprocess.run(["pdftotext", "-raw", str(pdf), str(text_file)], check=True)
    text = text_file.read_text()
    for label in ["Item", "Description", "Comment"]:
        actual = [int(i) for i in re.findall(label + r"\s+(\d{4})", text)]
        require(actual == list(range(groups)), f"{pages}: incomplete/reordered {label}")
    data = json.loads((CRATE / f"examples/variable-corpus-{pages}.json").read_text())["value"]
    compact = lambda value: "".join(value.split())
    pdf_compact = compact(text)
    for line in data["summary"]["value"].splitlines():
        require(compact(line) in pdf_compact, f"{pages}: missing supplied summary line")
    scene = json.loads((OUTPUT / f"variable-corpus-{pages}.json").read_text())
    for page in scene["pages"]:
        ids = [element["id"] for element in page["elements"]]
        require(ids.index("watermark") < ids.index("entries"), "background paints after body")
    for page in scene["pages"]:
        for element in page["elements"]:
            if element.get("text"):
                require(compact(element["text"]) in pdf_compact,
                        f"{pages}: incomplete PDF text in {element['id']}")
            table = element.get("table")
            if table:
                for row in table["rows"]:
                    for cell in row["cells"]:
                        require(compact(cell["text"]) in pdf_compact,
                                f"{pages}: incomplete PDF cell {row['source_index']}/{cell['field']}")
    verify_bars(pages, scene)
    return dict(pages=pages, items=groups, source_rows=groups * 3,
                sha256=hashlib.sha256(pdf.read_bytes()).hexdigest(), rendered_pages=pages,
                pdf_order_complete=True, supplied_totals_present=True, background_before_body=True,
                complete_scene_text_present=True,
                barcode_modules_verified=95, barcode_quiet_zones_verified=True)


def main():
    for command in ["pdfinfo", "pdftoppm", "pdftotext"]:
        require(shutil.which(command) is not None, f"required QA tool missing: {command}")
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    results = [verify_case(pages, groups) for pages, groups in CASES]
    (EVIDENCE / "pdf-content-and-bars.json").write_text(json.dumps(results, indent=2) + "\n")
    print("4 documents / 28 pages verified; inspect the page PNGs before approval")


if __name__ == "__main__":
    main()
