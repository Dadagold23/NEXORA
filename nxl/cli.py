from __future__ import annotations

import os
import sys

from .interpreter import Interpreter


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    if len(args) < 2:
        print("Usage: nxl run <file.nxl>")
        return 1

    command = args[0]
    if command == "run":
        file_path = args[1]
        if not os.path.exists(file_path):
            print(f"File not found: {file_path}")
            return 1
        interpreter = Interpreter()
        interpreter.run_file(file_path)
        return 0

    print(f"Unknown command: {command}")
    print("Usage: nxl run <file.nxl>")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
