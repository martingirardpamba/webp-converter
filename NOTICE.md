# Third-Party Notices

## FFmpeg

WebP Converter bundles a static build of **FFmpeg** to perform video conversion.

The bundled FFmpeg build includes `libx264` and `libvpx`, which makes that
binary licensed under the **GNU General Public License, version 2 or later
(GPLv2+)**. FFmpeg is a separate program invoked as an external binary; it is
aggregated with, not linked into, WebP Converter.

- FFmpeg project: https://ffmpeg.org
- FFmpeg source and license: https://ffmpeg.org/download.html
- The bundled Windows build is sourced from https://www.gyan.dev/ffmpeg/builds/

WebP Converter's own source code remains under the MIT License (see `LICENSE`).
The GPL applies to the bundled FFmpeg binary only.
