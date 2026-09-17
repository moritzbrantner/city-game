#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
import urllib.parse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES_PATH = ROOT / "fixtures/pages/osm-sources.json"
TRANSFORM_PATH = ROOT / "fixtures/osm-preserve-all-transform.json"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def import_scenario(source: Path, scenario: Path, receipt: Path) -> None:
    run(
        "cargo",
        "run",
        "--locked",
        "-p",
        "city-game-cli",
        "--",
        "import",
        str(source),
        str(scenario),
        str(TRANSFORM_PATH),
        str(receipt),
    )


def require_digest(label: str, actual: str, expected: str | None) -> None:
    if not expected:
        raise SystemExit(f"{label} is not pinned; run the explicit refresh workflow first")
    if actual != expected:
        raise SystemExit(f"{label} sha256 mismatch: expected {expected}, got {actual}")


def verify(metadata: dict) -> None:
    with tempfile.TemporaryDirectory(prefix="city-game-osm-verify-") as temp_dir:
        temp = Path(temp_dir)
        for source in metadata["sources"]:
            extract = ROOT / source["extractPath"]
            if not extract.is_file():
                raise SystemExit(f"missing pinned OSM extract: {extract}")
            require_digest(f"{source['id']} extract", sha256(extract), source.get("extractSha256"))

            generated_scenario = temp / f"{source['id']}-scenario.json"
            generated_receipt = temp / f"{source['id']}-receipt.json"
            import_scenario(extract, generated_scenario, generated_receipt)
            for generated, checked_in in (
                (generated_scenario, ROOT / source["scenarioPath"]),
                (generated_receipt, ROOT / source["receiptPath"]),
            ):
                if not checked_in.is_file() or generated.read_bytes() != checked_in.read_bytes():
                    raise SystemExit(f"{checked_in} is stale relative to the pinned OSM extract")


def refresh(metadata: dict) -> None:
    if shutil.which("osmium") is None:
        raise SystemExit("osmium is required for explicit OSM source refresh")

    osmium_version = subprocess.check_output(["osmium", "--version"], text=True).splitlines()[0].strip()
    recorded_version = metadata.get("osmiumVersion")
    if recorded_version and recorded_version != osmium_version:
        raise SystemExit(
            f"osmium version changed: expected {recorded_version!r}, got {osmium_version!r}; review before repinning"
        )
    metadata["osmiumVersion"] = osmium_version

    with tempfile.TemporaryDirectory(prefix="city-game-osm-refresh-") as temp_dir:
        temp = Path(temp_dir)
        downloaded: dict[str, Path] = {}
        source_digests: dict[str, str] = {}

        for source in metadata["sources"]:
            url = source["sourceUrl"]
            if url not in downloaded:
                filename = Path(urllib.parse.urlparse(url).path).name
                target = temp / filename
                run("curl", "--fail", "--location", "--retry", "3", "--output", str(target), url)
                actual_size = target.stat().st_size
                expected_size = int(source["sourceBytes"])
                if actual_size != expected_size:
                    raise SystemExit(
                        f"immutable source size mismatch for {url}: expected {expected_size}, got {actual_size}"
                    )
                downloaded[url] = target
                source_digests[url] = sha256(target)

            source_digest = source_digests[url]
            expected_source_digest = source.get("sourceSha256")
            if expected_source_digest and source_digest != expected_source_digest:
                raise SystemExit(
                    f"immutable source sha256 mismatch for {url}: expected {expected_source_digest}, got {source_digest}"
                )
            source["sourceSha256"] = source_digest

            extract = ROOT / source["extractPath"]
            extract.parent.mkdir(parents=True, exist_ok=True)
            bbox = ",".join(str(value) for value in source["bbox"])
            run(
                "osmium",
                "extract",
                "--strategy",
                "complete_ways",
                "--bbox",
                bbox,
                "--overwrite",
                "--output",
                str(extract),
                str(downloaded[url]),
            )
            source["extractSha256"] = sha256(extract)

            scenario = ROOT / source["scenarioPath"]
            receipt = ROOT / source["receiptPath"]
            scenario.parent.mkdir(parents=True, exist_ok=True)
            receipt.parent.mkdir(parents=True, exist_ok=True)
            import_scenario(extract, scenario, receipt)

    SOURCES_PATH.write_text(json.dumps(metadata, indent=2) + "\n")
    verify(metadata)


def main() -> None:
    parser = argparse.ArgumentParser(description="Refresh or verify pinned real OSM Pages scenarios")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--refresh", action="store_true", help="download immutable sources and repin outputs")
    mode.add_argument("--verify", action="store_true", help="verify checked-in extracts and canonical outputs offline")
    args = parser.parse_args()

    metadata = json.loads(SOURCES_PATH.read_text())
    if metadata.get("schemaVersion") != 1 or not isinstance(metadata.get("sources"), list):
        raise SystemExit("unsupported OSM source manifest")
    ids = [entry.get("id") for entry in metadata["sources"]]
    if sorted(ids) != sorted(set(ids)) or set(ids) != {"karlsruhe", "stuttgart", "pforzheim"}:
        raise SystemExit("OSM source manifest must contain exactly Karlsruhe, Stuttgart, and Pforzheim")

    if args.refresh:
        refresh(metadata)
    else:
        verify(metadata)


if __name__ == "__main__":
    main()
