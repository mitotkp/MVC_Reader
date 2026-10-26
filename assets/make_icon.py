"""Genera el icono de MVCreader (assets/icon.png e icon.ico).

Libro abierto con un renglón subrayado y una cinta de marcador, sobre un cuadrado
redondeado con degradado. Se dibuja a 1024 px y se reduce para que quede suave.
Uso: python assets/make_icon.py
"""
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 1024
OUT = Path(__file__).resolve().parent


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def quad(p0, c, p1, n=48):
    """Puntos de una curva cuadrática de Bézier."""
    pts = []
    for i in range(n + 1):
        t = i / n
        x = (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t * t * p1[0]
        y = (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t * t * p1[1]
        pts.append((x, y))
    return pts


def page(spine_top, spine_bot, outer_top, outer_bot, ctrl_top, ctrl_bot):
    return quad(spine_top, ctrl_top, outer_top) + quad(outer_bot, ctrl_bot, spine_bot)


def main():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    # Fondo: degradado diagonal índigo -> violeta con esquinas redondeadas.
    grad = Image.new("RGBA", (S, S))
    top, bottom = (67, 56, 202, 255), (124, 58, 237, 255)
    gp = grad.load()
    for y in range(S):
        for x in range(0, S, 4):
            c = lerp(top, bottom, (x * 0.35 + y * 0.65) / S)
            for k in range(4):
                gp[x + k, y] = c
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((40, 40, S - 40, S - 40), radius=210, fill=255)
    img.paste(grad, (0, 0), mask)

    # Brillo suave en la mitad superior.
    gloss = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(gloss).ellipse((-200, -620, S + 200, 470), fill=(255, 255, 255, 34))
    gloss.putalpha(ImageChops.multiply(gloss.getchannel("A"), mask))
    img = Image.alpha_composite(img, gloss)

    # Sombra del libro.
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    sd.polygon(page((512, 400), (512, 800), (170, 360), (170, 760), (340, 320), (340, 740)), fill=(20, 10, 60, 120))
    sd.polygon(page((512, 400), (512, 800), (854, 360), (854, 760), (684, 320), (684, 740)), fill=(20, 10, 60, 120))
    shadow = shadow.filter(ImageFilter.GaussianBlur(26))
    img = Image.alpha_composite(img, shadow)

    d = ImageDraw.Draw(img)
    # Grosor de las hojas (capa inferior gris clara).
    for dy, col in ((26, (205, 208, 225, 255)), (13, (226, 228, 240, 255))):
        d.polygon(page((512, 372 + dy), (512, 772 + dy), (182, 332 + dy), (182, 732 + dy), (346, 292 + dy), (346, 712 + dy)), fill=col)
        d.polygon(page((512, 372 + dy), (512, 772 + dy), (842, 332 + dy), (842, 732 + dy), (678, 292 + dy), (678, 712 + dy)), fill=col)

    # Páginas.
    left = page((512, 372), (512, 772), (182, 332), (182, 732), (346, 292), (346, 712))
    right = page((512, 372), (512, 772), (842, 332), (842, 732), (678, 292), (678, 712))
    d.polygon(left, fill=(250, 250, 255, 255))
    d.polygon(right, fill=(255, 255, 255, 255))

    # Sombra del lomo.
    for i in range(14):
        a = int(90 * (1 - i / 14))
        d.line(quad((512 - i, 374), (512 - i, 560), (512 - i, 770)), fill=(90, 80, 160, a), width=2)
        d.line(quad((512 + i, 374), (512 + i, 560), (512 + i, 770)), fill=(90, 80, 160, a // 2), width=2)

    # Renglones: siguen la curvatura de la página.
    line_col = (178, 184, 214, 255)
    rows = [0.2, 0.34, 0.48, 0.62, 0.76]
    for k, t in enumerate(rows):
        yl = 332 + (732 - 332) * t
        ys = 372 + (772 - 372) * t
        yc = 292 + (712 - 292) * t
        lw = 22
        d.line(quad((238, yl + 2), (346, yc + 8), (470, ys - 2), 32), fill=line_col, width=lw, joint="curve")
        if k == 2:
            # Renglón subrayado en amarillo en la página derecha.
            d.line(quad((540, ys - 2), (660, yc + 8), (742, yl + 6), 32), fill=(253, 216, 53, 255), width=58, joint="curve")
            d.line(quad((548, ys - 2), (660, yc + 8), (734, yl + 6), 32), fill=(150, 130, 60, 255), width=lw, joint="curve")
        else:
            end = 790 if k != 4 else 700
            d.line(quad((548, ys - 2), (678, yc + 8), (end, yl + 2), 32), fill=line_col, width=lw, joint="curve")

    # Cinta de marcador roja que cuelga de la página derecha.
    x0, x1, ytop, ybot = 764, 826, 300, 590
    d.polygon([(x0, ytop), (x1, ytop), (x1, ybot), ((x0 + x1) / 2, ybot - 46), (x0, ybot)], fill=(239, 68, 68, 255))
    d.polygon([(x0, ytop), (x0 + 16, ytop), (x0 + 16, ybot - 12), (x0, ybot)], fill=(200, 40, 50, 255))

    big = img.resize((256, 256), Image.LANCZOS)
    big.save(OUT / "icon.png")
    big.save(OUT / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("icon.png e icon.ico generados")


if __name__ == "__main__":
    main()
