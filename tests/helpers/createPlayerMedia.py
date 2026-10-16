"""Generate small, fictional MJPEG/dual-PCM AVI + SRT fixtures. No user media."""
import io
import math
from pathlib import Path
import struct
import sys
from PIL import Image, ImageDraw

def chunk(kind, data):
    return kind + struct.pack('<I', len(data)) + data + b'\0' * (len(data) % 2)

def listing(kind, data):
    return chunk(b'LIST', kind + data)

def video(path, background, long=False):
    fps, frames, width, height = (1, 7200, 160, 96) if long else (20, 1200, 160, 96)
    rate = 1000 if long else 8000
    samples = rate // fps
    header = struct.pack('<14I', 1000000 // fps, 1000000, 0, 0x10, frames, 0, 3, 0, width, height, 0, 0, 0, 0)
    sh = struct.pack('<4s4sIHHIIIIIIIIhhhh', b'vids', b'MJPG', 0, 0, 0, 0, 1, fps, 0, frames, 65536, 0xffffffff, 0, 0, 0, width, height)
    sf = struct.pack('<IiiHH4sIiiII', 40, width, height, 1, 24, b'MJPG', width * height * 3, 0, 0, 0, 0)
    streams = listing(b'strl', chunk(b'strh', sh) + chunk(b'strf', sf))
    for _ in range(2):
        ah = struct.pack('<4s4sIHHIIIIIIIIhhhh', b'auds', b'\0'*4, 0, 0, 0, 0, 2, rate * 2, 0, rate * frames // fps, samples * 2, 0xffffffff, 2, 0, 0, 0, 0)
        af = struct.pack('<HHIIHH', 1, 1, rate, rate * 2, 2, 16)
        streams += listing(b'strl', chunk(b'strh', ah) + chunk(b'strf', af))
    movi, index = bytearray(), bytearray()
    for n in range(frames):
        image = Image.new('RGB', (width, height), background)
        draw = ImageDraw.Draw(image)
        draw.rectangle((int(n % 120), 30, int(n % 120) + 30, 70), fill='#d7bb80')
        draw.text((8, 8), f'THE ARCHIVE  {n / fps:.1f}s', fill='white')
        buffer = io.BytesIO(); image.save(buffer, 'JPEG')
        audio = [bytes(samples * 2)] * 2 if long else [b''.join(struct.pack('<h', int(200 * math.sin(2 * math.pi * hz * (n * samples + i) / rate))) for i in range(samples)) for hz in (330, 440)]
        for kind, body in [(b'00dc', buffer.getvalue()), (b'01wb', audio[0]), (b'02wb', audio[1])]:
            index += struct.pack('<4sIII', kind, 0x10, len(movi) + 4, len(body))
            movi += chunk(kind, body)
    body = b'AVI ' + listing(b'hdrl', chunk(b'avih', header) + streams) + listing(b'movi', movi) + chunk(b'idx1', index)
    path.write_bytes(chunk(b'RIFF', body))
    end = '01:59:59,500' if long else '00:00:59,500'
    path.with_suffix('.srt').write_text(f'1\n00:00:00,000 --> {end}\n合成字幕 · TheArchive 播放验证\n', encoding='utf-8')

if __name__ == '__main__':
    root = Path(sys.argv[1]); root.mkdir(parents=True, exist_ok=True)
    long = '--long' in sys.argv[2:]
    video(root / '第 1集 演示.avi', '#19394b', long)
    video(root / '第 2集 演示.avi', '#3f2444', long)
    print('Created 2 fictional videos, each with 2 audio tracks and matching SRT subtitles.')
