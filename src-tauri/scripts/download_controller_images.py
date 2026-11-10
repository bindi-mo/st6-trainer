#!/usr/bin/env python3
"""
Controller Image Downloader

This script downloads controller button and directional input images from
the official Street Fighter 6 website and saves them to a local directory.
It uses a key-to-symbol mapping dictionary, supports normalized output
directory paths via pathlib, and includes options to overwrite existing files.
"""

__version__ = "1.1.3"

import argparse
import os
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Iterable
from pathlib import Path

from fake_useragent import UserAgent
from sf6_frame_data_processor import IMG_MAPPING

ASSET_URL = "https://www.streetfighter.com/6/assets/"
CONTROLER_PATH = "images/common/controller"


def safe_urljoin(base, path):
    """
    Adjust the end of the base path only if the path (the second argument)
     is a "pure relative path" that does not begin with '/' or 'http'.
    """
    if (
        not base.endswith("/")
        and not path.startswith("/")
        and not path.startswith("http")
    ):
        base += "/"
    return urllib.parse.urljoin(base, path)


def download_mapping_images(
    base_url: str,
    output_dir: Path,
    img_mapping: dict[str, str],
    overwrite: bool = False,
) -> None:
    """
    Downloads image files specified in the keys of img_mapping and
    saves them into output_dir.

    :param base_url: The base URL where images are hosted.
    :param img_mapping: Dictionary mapping image filenames to notation symbols.
    :param output_dir: The target Path directory to save downloaded images.
    :param overwrite: If True, overwrite existing files. If False, skip existing files.
    """
    # Set User-Agent to avoid HTTP 403 Forbidden errors
    ua = UserAgent()
    headers = {
        # "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        "User-Agent": ua.random
    }

    success_count = 0
    fail_count = 0

    for filename, symbol in img_mapping.items():
        image_url = safe_urljoin(base_url, filename)
        save_path = output_dir / filename

        if save_path.exists() and overwrite is False:
            print(f"[{symbol}] Skip: {filename} ... ", end="\n")
            continue

        print(f"[{symbol}] Downloading: {filename} ... ", end="")

        try:
            # Create request object with headers
            req = urllib.request.Request(image_url, headers=headers)
            with urllib.request.urlopen(req) as response:
                if response.status == 200:
                    with open(save_path, "wb") as f:
                        f.write(response.read())
                    print("SUCCESS")
                    success_count += 1
                else:
                    print(f"FAILED (HTTP Status: {response.status})")
                    fail_count += 1

        except urllib.error.HTTPError as e:
            print(f"FAILED (HTTP Error: {e.code})")
            fail_count += 1
        except urllib.error.URLError as e:
            print(f"FAILED (URL Error: {e.reason})")
            fail_count += 1
        except Exception as e:  # ruff: ignore[BLE001]
            print(f"FAILED (Error: {e})")
            fail_count += 1

    print("\n--- Download Summary ---")
    print(f"Success: {success_count} / Failed: {fail_count}")


def parse_arg(args: Iterable[str] | None = None):
    """
    Parses command line arguments.
    """
    parser = argparse.ArgumentParser(
        description=f"Download controller notation image assets. (v{__version__})",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "-o",
        "--output",
        type=str,
        default="./controller_images",
        help="Directory where downloaded images will be saved.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        default=False,
        help="Force overwrite downloaded images files.",
    )

    return parser.parse_args(args=args)


def main(args: Iterable[str] | None = None):
    target_url = urllib.parse.urljoin(ASSET_URL, CONTROLER_PATH)
    output_path = Path(os.path.abspath(__file__)).parent / args.output

    # Create output directory if it does not exist using pathlib.Path
    output_path.mkdir(parents=True, exist_ok=True)
    print(f"Output Directory: {output_path.resolve()}\n")

    download_mapping_images(
        target_url, output_path.resolve(), IMG_MAPPING, args.force
    )


if __name__ == "__main__":
    ARGS = parse_arg()
    main(ARGS)
