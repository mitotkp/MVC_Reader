"""Genera los documentos de ejemplo de MVCreader (una guía del propio programa).

Salida en samples/: guia-mvcreader.epub, mvcreader-guide.epub, guia-mvcreader.pdf
y notas-de-version.docx.
El PDF se imprime con Microsoft Edge en modo headless.
Uso: python samples/make_samples.py
"""
import html
import subprocess
import tempfile
import zipfile
from pathlib import Path

OUT = Path(__file__).resolve().parent
EDGE = Path(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")

ES = {
    "lang": "es",
    "title": "Guía de MVCreader",
    "author": "Proyecto MVCreader",
    "chapters": [
        ("Bienvenida", [
            "MVCreader es un lector de documentos para Windows pensado para leer con calma: abre archivos PDF, EPUB y DOCX en una sola ventana, recuerda dónde te quedaste y guarda tus subrayados y notas sin depender de ninguna cuenta ni conexión a internet.",
            "Esta guía es, al mismo tiempo, un documento de ejemplo. Puedes abrirla con el programa para probar cada función mientras lees: pasar de página, subrayar este mismo párrafo, dejar una nota o buscar una palabra.",
            "El programa está escrito en Rust y dibuja su interfaz directamente con la tarjeta de video. No usa un navegador por dentro, por eso arranca rápido y ocupa poca memoria comparado con otras aplicaciones de escritorio modernas.",
        ]),
        ("Abrir documentos", [
            "Hay tres formas de abrir un archivo: con el botón «Abrir», con el atajo Ctrl+O o arrastrando el archivo desde el Explorador de Windows hasta la ventana. También puedes usar «Abrir con…» y elegir MVCreader.",
            "La pantalla de inicio muestra los documentos recientes como tarjetas. Cada tarjeta indica el formato, la ruta del archivo, el porcentaje leído y la fecha en que lo abriste por última vez.",
            "Los datos de cada documento se asocian a su contenido y no a su nombre. Si mueves o renombras el archivo, MVCreader lo reconoce igual y conserva tu página, tus marcadores y tus notas.",
        ]),
        ("Leer y navegar", [
            "Para cambiar de página usa las flechas del teclado, la barra espaciadora, la rueda del ratón o las flechas redondas que aparecen a los lados de la hoja. Inicio y Fin te llevan al principio y al final.",
            "En la barra superior verás la página actual y el total. Escribe un número y pulsa Enter para saltar directamente; el atajo Ctrl+G pone el cursor en ese campo.",
            "Los libros EPUB y los documentos de Word se paginan según el tamaño de la ventana y la letra elegida. Si cambias el tamaño de letra, el texto se vuelve a acomodar y te mantiene en el mismo punto de la lectura.",
            "El índice del panel lateral resalta el capítulo en el que estás. Haz clic en cualquier entrada para ir a ella.",
        ]),
        ("Subrayar", [
            "Para subrayar, arrastra el ratón sobre el texto. Al soltar aparece un pequeño menú con cuatro colores: amarillo, verde, azul y rosa. Elige uno y el subrayado queda guardado.",
            "Si haces clic sobre un subrayado existente puedes cambiar su color, escribirle una nota, copiar el texto o quitarlo. Todos los subrayados aparecen también en la pestaña correspondiente del panel lateral.",
            "Los subrayados funcionan igual en PDF que en EPUB y DOCX. En los PDF se dibujan exactamente sobre las letras de la página original.",
        ]),
        ("Marcadores y notas", [
            "Pulsa Ctrl+D o la cinta roja de la esquina de la hoja para marcar una página. Los marcadores se listan en el panel lateral y puedes cambiarles el nombre.",
            "Las notas pueden ser generales, sobre el documento completo, o de una página concreta. Cuando una página tiene notas, aparece un pequeño aviso amarillo en su esquina.",
            "Con el botón «Exportar notas y subrayados» obtienes un archivo Markdown ordenado por páginas, listo para estudiar o compartir.",
        ]),
        ("Buscar", [
            "El buscador se abre con Ctrl+F. No distingue mayúsculas ni tildes: buscar «pagina» encuentra también «Página».",
            "Las coincidencias se resaltan en la hoja y se listan con su contexto en el panel lateral. Enter o F3 llevan a la siguiente; Mayús+F3 a la anterior.",
            "En los PDF grandes la búsqueda avanza por partes, así que la ventana nunca se congela aunque el documento tenga cientos de páginas.",
        ]),
        ("Ajustes", [
            "Desde el botón de engranaje eliges el tema claro, sepia u oscuro, el idioma de la interfaz (español o inglés) y el tamaño general de los controles.",
            "Para EPUB y DOCX puedes ajustar el tamaño y el tipo de letra, el interlineado, el ancho de la columna de texto y si los párrafos se justifican. Una vista previa muestra el resultado al instante.",
            "En el tema oscuro las páginas PDF se invierten para no deslumbrar; si prefieres ver los colores originales, puedes desactivarlo.",
        ]),
    ],
}

EN = {
    "lang": "en",
    "title": "MVCreader Guide",
    "author": "MVCreader project",
    "chapters": [
        ("Welcome", [
            "MVCreader is a document reader for Windows made for calm reading: it opens PDF, EPUB and DOCX files in a single window, remembers where you stopped and keeps your highlights and notes without any account or internet connection.",
            "This guide is also a sample document. Open it with the program to try every feature while you read: turn pages, highlight this very paragraph, leave a note or search for a word.",
            "The program is written in Rust and draws its interface directly on the graphics card. There is no web browser inside, so it starts fast and uses little memory compared with other modern desktop apps.",
        ]),
        ("Opening documents", [
            "There are three ways to open a file: the Open button, the Ctrl+O shortcut, or dragging the file from Windows Explorer onto the window. You can also use Open with… and pick MVCreader.",
            "The start screen shows recent documents as cards. Each card shows the format, the file path, how much you have read and when you last opened it.",
            "Each document's data is tied to its content, not its name. If you move or rename the file, MVCreader still recognises it and keeps your page, bookmarks and notes.",
        ]),
        ("Reading and navigating", [
            "Turn pages with the arrow keys, the space bar, the mouse wheel or the round arrows beside the page. Home and End take you to the beginning and the end.",
            "The top bar shows the current page and the total. Type a number and press Enter to jump straight to it; Ctrl+G puts the cursor in that box.",
            "EPUB books and Word documents are paginated to fit the window and the chosen font. If you change the font size the text reflows and keeps you at the same point.",
            "The table of contents in the side panel highlights the chapter you are in. Click any entry to go there.",
        ]),
        ("Highlighting", [
            "To highlight, drag the mouse over the text. When you release it a small menu offers four colors: yellow, green, blue and pink. Pick one and the highlight is saved.",
            "Click an existing highlight to change its color, write a note, copy the text or remove it. All highlights are also listed in their tab of the side panel.",
            "Highlights work the same in PDF, EPUB and DOCX. In PDF files they are drawn exactly over the letters of the original page.",
        ]),
        ("Bookmarks and notes", [
            "Press Ctrl+D or the red ribbon in the page corner to bookmark a page. Bookmarks are listed in the side panel and can be renamed.",
            "Notes can be general, about the whole document, or tied to a page. When a page has notes, a small yellow badge appears in its corner.",
            "The Export notes and highlights button produces a Markdown file sorted by page, ready to study or share.",
        ]),
        ("Searching", [
            "Search opens with Ctrl+F. It ignores case and accents, so searching for cafe also finds Café.",
            "Matches are highlighted on the page and listed with their context in the side panel. Enter or F3 go to the next one; Shift+F3 to the previous one.",
            "In large PDF files the search runs in small steps, so the window never freezes even with hundreds of pages.",
        ]),
        ("Settings", [
            "The gear button lets you choose the light, sepia or dark theme, the interface language (Spanish or English) and the overall size of the controls.",
            "For EPUB and DOCX you can set the font size and typeface, the line spacing, the width of the text column and whether paragraphs are justified. A preview shows the result instantly.",
            "In the dark theme PDF pages are inverted so they do not dazzle; turn it off if you prefer the original colors.",
        ]),
    ],
}

CSS = """
body { font-family: Georgia, serif; line-height: 1.55; }
h1 { font-family: 'Segoe UI', sans-serif; }
h2 { font-family: 'Segoe UI', sans-serif; color: #4338ca; }
"""


def chapter_xhtml(lang, n, title, paras):
    body = "\n".join(f"<p>{html.escape(p)}</p>" for p in paras)
    return f"""<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="{lang}">
<head><title>{html.escape(title)}</title><link rel="stylesheet" href="style.css"/></head>
<body><section id="c{n}"><h2>{n}. {html.escape(title)}</h2>
{body}
</section></body></html>"""


def make_epub(doc, path):
    chapters = doc["chapters"]
    items = "\n".join(
        f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(1, len(chapters) + 1)
    )
    spine = "\n".join(f'<itemref idref="c{i}"/>' for i in range(1, len(chapters) + 1))
    nav_items = "\n".join(
        f'<li><a href="c{i}.xhtml">{i}. {html.escape(t)}</a></li>' for i, (t, _) in enumerate(chapters, 1)
    )
    opf = f"""<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="id">mvcreader-sample-{doc['lang']}</dc:identifier>
<dc:title>{html.escape(doc['title'])}</dc:title>
<dc:creator>{html.escape(doc['author'])}</dc:creator>
<dc:language>{doc['lang']}</dc:language>
</metadata>
<manifest>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="css" href="style.css" media-type="text/css"/>
<item id="title" href="title.xhtml" media-type="application/xhtml+xml"/>
{items}
</manifest>
<spine>
<itemref idref="title"/>
{spine}
</spine>
</package>"""
    nav = f"""<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>{html.escape(doc['title'])}</title></head>
<body><nav epub:type="toc"><ol>{nav_items}</ol></nav></body></html>"""
    title = f"""<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{html.escape(doc['title'])}</title></head>
<body><h1>{html.escape(doc['title'])}</h1><p><i>{html.escape(doc['author'])}</i></p></body></html>"""
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr(
            "META-INF/container.xml",
            '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
            '<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
            compress_type=zipfile.ZIP_DEFLATED,
        )
        z.writestr("OEBPS/content.opf", opf, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/nav.xhtml", nav, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/style.css", CSS, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/title.xhtml", title, compress_type=zipfile.ZIP_DEFLATED)
        for i, (t, paras) in enumerate(chapters, 1):
            z.writestr(f"OEBPS/c{i}.xhtml", chapter_xhtml(doc["lang"], i, t, paras), compress_type=zipfile.ZIP_DEFLATED)


def make_pdf(doc, path):
    parts = [f"<h1>{html.escape(doc['title'])}</h1><p><i>{html.escape(doc['author'])}</i></p>"]
    for i, (t, paras) in enumerate(doc["chapters"], 1):
        parts.append(f"<h2>{i}. {html.escape(t)}</h2>")
        parts.extend(f"<p>{html.escape(p)}</p>" for p in paras)
    page = f"""<!DOCTYPE html><html lang="{doc['lang']}"><head><meta charset="utf-8"><title>{html.escape(doc['title'])}</title>
<style>@page {{ size: A5; margin: 18mm 16mm; }} {CSS} body {{ font-size: 11pt; text-align: justify; }}
h1 {{ font-size: 24pt; margin-top: 30mm; }} h2 {{ break-before: page; font-size: 16pt; }}</style></head>
<body>{''.join(parts)}</body></html>"""
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "guia.html"
        src.write_text(page, encoding="utf-8")
        subprocess.run(
            [str(EDGE), "--headless=new", "--disable-gpu", "--no-pdf-header-footer", f"--print-to-pdf={path}", src.as_uri()],
            check=True,
            timeout=120,
        )


def make_docx(doc, path):
    """DOCX mínimo: título, capítulos con estilo «heading 1» y párrafos."""
    def para(text, style=None):
        ppr = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
        return f'<w:p>{ppr}<w:r><w:t xml:space="preserve">{html.escape(text)}</w:t></w:r></w:p>'

    body = [para(doc["title"], "Title")]
    for i, (t, paras) in enumerate(doc["chapters"], 1):
        body.append(para(f"{i}. {t}", "Heading1"))
        body.extend(para(p) for p in paras)
    w = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
    document = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {w}><w:body>{"".join(body)}</w:body></w:document>'
    styles = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {w}>'
        '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/></w:style>'
        '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>'
        "</w:styles>"
    )
    ct = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    )
    doc_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        "</Relationships>"
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", ct)
        z.writestr("_rels/.rels", rels)
        z.writestr("word/_rels/document.xml.rels", doc_rels)
        z.writestr("word/document.xml", document)
        z.writestr("word/styles.xml", styles)


def main():
    make_epub(ES, OUT / "guia-mvcreader.epub")
    make_epub(EN, OUT / "mvcreader-guide.epub")
    make_docx(dict(ES, title="Notas de la versión 0.1"), OUT / "notas-de-version.docx")
    if EDGE.exists():
        make_pdf(ES, OUT / "guia-mvcreader.pdf")
    print("Ejemplos generados en", OUT)


if __name__ == "__main__":
    main()
