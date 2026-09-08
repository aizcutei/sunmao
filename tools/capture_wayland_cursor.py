#!/usr/bin/env python3
"""Read the nested compositor's pixels; the editor itself never opens X11."""
import ctypes as c
import os
import sys

x11 = c.CDLL("libX11.so.6")
x11.XOpenDisplay.argtypes = [c.c_char_p]
x11.XOpenDisplay.restype = c.c_void_p
x11.XGetImage.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int,
                          c.c_uint, c.c_uint, c.c_ulong, c.c_int]
x11.XGetImage.restype = c.c_void_p
x11.XGetPixel.argtypes = [c.c_void_p, c.c_int, c.c_int]
x11.XGetPixel.restype = c.c_ulong
x11.XDestroyImage.argtypes = [c.c_void_p]
x11.XCloseDisplay.argtypes = [c.c_void_p]
display = x11.XOpenDisplay(os.environ["SUNMAO_INPUT_DISPLAY"].encode())
if not display:
    raise RuntimeError("Cannot connect the screenshot observer to Xvfb")
try:
    # CI fixes Xvfb to a 24-bit TrueColor screen and Weston to 640x480.
    image = x11.XGetImage(display, int(os.environ["SUNMAO_INPUT_WINDOW"]),
                          288, 208, 64, 64, c.c_ulong(-1).value, 2)
    if not image:
        raise RuntimeError("Cannot capture the nested compositor")
    try:
        pixels = bytearray()
        for y in range(64):
            for x in range(64):
                value = x11.XGetPixel(image, x, y)
                pixels.extend(((value >> 16) & 255, (value >> 8) & 255, value & 255))
        sys.stdout.buffer.write(pixels)
    finally:
        x11.XDestroyImage(image)
finally:
    x11.XCloseDisplay(display)
