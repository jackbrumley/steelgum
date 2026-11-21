"""Validate the LibGodot probe's binary frame stream independently of Godot."""

import argparse
import struct
from pathlib import Path


def validate(data):
    if not data.startswith(b"SGRGBA01"):
        raise ValueError("expected the version 1 RGBA stream header")
    offset = 8
    frames = []
    for expected_index in range(2):
        if len(data) < offset + 12:
            raise ValueError("incomplete frame header")
        index, width, height = struct.unpack_from("<III", data, offset)
        offset += 12
        if (index, width, height) != (expected_index, 256, 160):
            raise ValueError(f"unexpected frame header {(index, width, height)}")
        end = offset + width * height * 4
        if len(data) < end:
            raise ValueError("incomplete pixel data")
        frame = data[offset:end]
        pixels = zip(frame[0::4], frame[1::4], frame[2::4])
        orange = sum(r > 90 and r > g + 35 and g > b + 12 for r, g, b in pixels)
        if orange < 100:
            raise ValueError(f"frame {index} does not show the orange 3D fixture")
        frames.append(frame)
        offset = end
    if len(data) != offset:
        raise ValueError("unexpected bytes after the second frame")
    if sum(a != b for a, b in zip(*frames)) < 500:
        raise ValueError("the two camera frames look the same")
    return frames


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stream", type=Path)
    args = parser.parse_args()
    try:
        validate(args.stream.read_bytes())
    except (OSError, ValueError) as error:
        parser.exit(1, f"LibGodot frame check: {error}\n")
    print("Frame stream contains two distinct 256×160 RGBA frames.")


if __name__ == "__main__":
    main()
