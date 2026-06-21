"""
generate_fixtures.py — Synthetic French legal VLM-OCR evaluation fixtures
==========================================================================
Generates 300 DPI-equivalent PNG files in four classes:
  printed/    — clean typeset contract clauses
  handwritten/— short handwritten-style annotations
  tables/     — structured data tables
  stamps/     — text blocks with CACHET / REÇU overlays

ALL content is entirely fictional.  No real PII, no real case numbers,
no real company names, no real addresses.

Usage (from this directory):
    python generate_fixtures.py

Requirements:
    Pillow >= 9.0  (pip install Pillow)
"""

from __future__ import annotations

import math
import os
import textwrap
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# ---------------------------------------------------------------------------
# Layout constants
# ---------------------------------------------------------------------------
DPI = 300
A4_W = int(8.27 * DPI)   # 2481 px
A4_H = int(11.69 * DPI)  # 3507 px

MARGIN = int(1.0 * DPI)   # 1 inch margin
TEXT_W = A4_W - 2 * MARGIN

BG_WHITE = (255, 255, 255)
FG_BLACK = (10, 10, 10)
FG_DARK  = (30, 30, 30)
FG_STAMP = (180, 30, 30)

SCRIPT_DIR = Path(__file__).parent


# ---------------------------------------------------------------------------
# Font helpers
# ---------------------------------------------------------------------------

def _get_font(size: int, bold: bool = False) -> ImageFont.ImageFont:
    """Return a PIL font.  Falls back gracefully to the built-in bitmap font."""
    candidates_regular = [
        "C:/Windows/Fonts/times.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
        "/Library/Fonts/Times New Roman.ttf",
    ]
    candidates_bold = [
        "C:/Windows/Fonts/timesbd.ttf",
        "C:/Windows/Fonts/arialbd.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf",
        "/Library/Fonts/Times New Roman Bold.ttf",
    ]
    candidates = candidates_bold if bold else candidates_regular
    for path in candidates:
        if os.path.isfile(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                pass
    return ImageFont.load_default()


def _get_mono_font(size: int) -> ImageFont.ImageFont:
    candidates = [
        "C:/Windows/Fonts/cour.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/Library/Fonts/Courier New.ttf",
    ]
    for path in candidates:
        if os.path.isfile(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                pass
    return ImageFont.load_default()


# ---------------------------------------------------------------------------
# Drawing helpers
# ---------------------------------------------------------------------------

def _new_page() -> tuple[Image.Image, ImageDraw.ImageDraw]:
    img = Image.new("RGB", (A4_W, A4_H), BG_WHITE)
    draw = ImageDraw.Draw(img)
    return img, draw


def _wrap_draw(draw: ImageDraw.ImageDraw, text: str, x: int, y: int,
               font: ImageFont.ImageFont, fill=FG_BLACK,
               max_width: int = TEXT_W) -> int:
    """Word-wrap *text* and draw it.  Returns the y position after the block."""
    lines = []
    for paragraph in text.split("\n"):
        if paragraph.strip() == "":
            lines.append("")
            continue
        wrapped = textwrap.wrap(paragraph, width=70)
        lines.extend(wrapped if wrapped else [""])

    line_h = font.size + int(font.size * 0.4)
    for line in lines:
        draw.text((x, y), line, font=font, fill=fill)
        y += line_h
    return y


# ---------------------------------------------------------------------------
# Class generators
# ---------------------------------------------------------------------------

def _make_printed(out_dir: Path) -> None:
    """3 × clean typeset contract clause images + .ref.txt sidecars."""

    articles = [
        {
            "file": "printed_01_objet_contrat",
            "ref": (
                "CONTRAT DE PRESTATION DE SERVICES\n"
                "Entre les soussignés :\n"
                "SARL Fictive, société à responsabilité limitée au capital de 10 000 euros, "
                "dont le siège social est situé au 12 rue des Lilas, 75001 Paris (fictif), "
                "immatriculée au RCS de Paris sous le numéro 123 456 789 (fictif), "
                "représentée par M. Jean Martin, agissant en qualité de Gérant,\n"
                "ci-après dénommée « le Prestataire »,\n"
                "ET\n"
                "Mme Marie Dupont, demeurant au 5 allée des Roses, 69002 Lyon (fictif),\n"
                "ci-après dénommée « le Client ».\n\n"
                "Article 1 – Objet du contrat\n"
                "Le présent contrat a pour objet de définir les conditions dans lesquelles "
                "le Prestataire s'engage à fournir au Client des services de conseil juridique "
                "en droit des contrats, tels que décrits à l'Annexe A ci-jointe.\n\n"
                "Article 2 – Durée\n"
                "Le contrat est conclu pour une durée déterminée de douze (12) mois à compter "
                "de la date de signature des présentes, soit du 1er janvier 2025 au 31 décembre 2025."
            ),
        },
        {
            "file": "printed_02_confidentialite",
            "ref": (
                "Article 5 – Confidentialité\n"
                "Chacune des parties s'engage à garder strictement confidentiels tous les "
                "documents, informations, données et savoir-faire communiqués par l'autre "
                "partie dans le cadre de l'exécution du présent contrat, et désignés comme "
                "confidentiels au moment de leur communication.\n\n"
                "Cette obligation de confidentialité s'applique pendant toute la durée du "
                "contrat et pendant une période de cinq (5) ans à compter de son expiration "
                "ou de sa résiliation, quelle qu'en soit la cause.\n\n"
                "En cas de violation de la présente clause, la partie fautive sera tenue de "
                "réparer l'intégralité du préjudice subi par l'autre partie, sans préjudice "
                "des autres recours dont celle-ci pourrait disposer.\n\n"
                "Article 6 – Propriété intellectuelle\n"
                "L'ensemble des travaux, études, rapports et livrables produits par le "
                "Prestataire dans le cadre du contrat restent la propriété exclusive du "
                "Client dès lors qu'ils ont été intégralement réglés par ce dernier."
            ),
        },
        {
            "file": "printed_03_responsabilite",
            "ref": (
                "Article 8 – Limitation de responsabilité\n"
                "La responsabilité du Prestataire ne peut être engagée qu'en cas de faute "
                "prouvée. Elle est expressément limitée aux dommages directs et prévisibles "
                "résultant de l'inexécution fautive du contrat.\n\n"
                "En tout état de cause, la responsabilité totale du Prestataire au titre "
                "du présent contrat est plafonnée au montant des sommes effectivement perçues "
                "par le Prestataire au cours des six (6) mois précédant le fait générateur "
                "du dommage.\n\n"
                "Le Prestataire ne saurait être tenu responsable des dommages indirects tels "
                "que perte d'exploitation, perte de chiffre d'affaires, perte de données ou "
                "atteinte à l'image.\n\n"
                "Article 9 – Droit applicable et juridiction compétente\n"
                "Le présent contrat est soumis au droit français. En cas de litige, "
                "les parties s'engagent à rechercher une solution amiable avant toute "
                "action judiciaire. À défaut d'accord amiable, le litige sera soumis "
                "au Tribunal de Commerce de Paris."
            ),
        },
    ]

    font_title = _get_font(52, bold=True)
    font_body  = _get_font(40)

    for art in articles:
        img, draw = _new_page()
        lines = art["ref"].split("\n")
        y = MARGIN

        for line in lines:
            if not line.strip():
                y += int(40 * 0.6)
                continue
            # Article headings get bold
            is_heading = line.startswith("Article") or line.isupper() or line.startswith("Entre") or line.startswith("ET\n")
            font = font_title if (line.startswith("Article") or line.isupper()) else font_body
            y = _wrap_draw(draw, line, MARGIN, y, font)

        path = out_dir / f"{art['file']}.png"
        img.save(str(path), dpi=(DPI, DPI))
        ref_path = out_dir / f"{art['file']}.ref.txt"
        ref_path.write_text(art["ref"], encoding="utf-8")
        print(f"  written: {path.name}")


def _make_handwritten(out_dir: Path) -> None:
    """2 × simulated handwritten annotation images + .ref.txt sidecars."""

    samples = [
        {
            "file": "handwritten_01_lu_approuve",
            "ref": "Lu et approuvé – Jean Martin\nFait à Paris, le 15 mars 2025",
        },
        {
            "file": "handwritten_02_bon_pour_accord",
            "ref": "Bon pour accord\nMarie Dupont\nLyon, 20 avril 2025",
        },
    ]

    # Slightly slanted mono font simulates handwriting
    font = _get_mono_font(72)

    for s in samples:
        img, draw = _new_page()

        # Light-cream background to simulate aged paper
        img.paste((252, 248, 232), [0, 0, A4_W, A4_H])
        draw = ImageDraw.Draw(img)

        # Faint ruled lines
        for row in range(MARGIN, A4_H - MARGIN, int(0.6 * DPI)):
            draw.line([(MARGIN, row), (A4_W - MARGIN, row)], fill=(200, 195, 180), width=2)

        y = MARGIN + int(DPI * 1.5)
        for line in s["ref"].split("\n"):
            # Slight x-jitter to fake pen strokes
            x_offset = MARGIN + int(0.1 * DPI) + (hash(line) % 20 - 10)
            draw.text((x_offset, y), line, font=font, fill=(20, 20, 80))
            y += int(font.size * 1.6)

        path = out_dir / f"{s['file']}.png"
        img.save(str(path), dpi=(DPI, DPI))
        ref_path = out_dir / f"{s['file']}.ref.txt"
        ref_path.write_text(s["ref"], encoding="utf-8")
        print(f"  written: {path.name}")


def _make_tables(out_dir: Path) -> None:
    """2 × table images with French legal data + .ref.txt sidecars."""

    tables = [
        {
            "file": "table_01_parties",
            "title": "Tableau des parties contractantes",
            "headers": ["Qualité", "Nom / Raison sociale", "Représentant", "Date de naissance"],
            "rows": [
                ["Prestataire", "SARL Fictive",     "M. Jean Martin",    "15/06/1978"],
                ["Client",     "Mme Marie Dupont", "—",                  "22/11/1985"],
                ["Garant",     "SAS Garantie FR",  "M. Paul Lefebvre",  "03/04/1960"],
            ],
        },
        {
            "file": "table_02_echeancier",
            "title": "Échéancier des paiements",
            "headers": ["Échéance", "Date limite", "Montant (€ HT)", "Statut"],
            "rows": [
                ["Acompte (30 %)", "01/02/2025", "3 000,00",  "Réglé"],
                ["Solde 1 (40 %)", "01/05/2025", "4 000,00",  "En attente"],
                ["Solde 2 (30 %)", "01/09/2025", "3 000,00",  "En attente"],
                ["Total",          "—",          "10 000,00", "—"],
            ],
        },
    ]

    font_title = _get_font(48, bold=True)
    font_head  = _get_font(36, bold=True)
    font_cell  = _get_font(34)

    for tbl in tables:
        img, draw = _new_page()
        y = MARGIN

        # Title
        draw.text((MARGIN, y), tbl["title"], font=font_title, fill=FG_BLACK)
        y += font_title.size + int(font_title.size * 0.6)

        cols = len(tbl["headers"])
        col_w = TEXT_W // cols
        row_h = int(0.45 * DPI)
        pad = int(0.08 * DPI)

        def draw_row(row_data: list[str], fy: int, header: bool = False) -> int:
            bg = (220, 230, 245) if header else BG_WHITE
            for ci, cell in enumerate(row_data):
                x0 = MARGIN + ci * col_w
                y0 = fy
                x1 = x0 + col_w
                y1 = fy + row_h
                draw.rectangle([x0, y0, x1, y1], fill=bg, outline=FG_DARK, width=2)
                font = font_head if header else font_cell
                draw.text((x0 + pad, y0 + pad), cell, font=font, fill=FG_BLACK)
            return fy + row_h

        y = draw_row(tbl["headers"], y, header=True)
        for row in tbl["rows"]:
            y = draw_row(row, y)

        # Build plain-text reference
        sep = " | "
        ref_lines = [tbl["title"], ""]
        ref_lines.append(sep.join(tbl["headers"]))
        ref_lines.append("-" * (sum(len(h) for h in tbl["headers"]) + len(sep) * (cols - 1)))
        for row in tbl["rows"]:
            ref_lines.append(sep.join(row))
        ref_text = "\n".join(ref_lines)

        path = out_dir / f"{tbl['file']}.png"
        img.save(str(path), dpi=(DPI, DPI))
        ref_path = out_dir / f"{tbl['file']}.ref.txt"
        ref_path.write_text(ref_text, encoding="utf-8")
        print(f"  written: {path.name}")


def _make_stamps(out_dir: Path) -> None:
    """2 × text-with-stamp overlay images + .ref.txt sidecars."""

    samples = [
        {
            "file": "stamp_01_recu",
            "body_ref": (
                "Reçu de la somme de dix mille euros (10 000,00 €)\n"
                "en règlement de la facture n° FACT-2025-0042 (fictif)\n"
                "émise par SARL Fictive à l'attention de Mme Marie Dupont.\n"
                "Fait à Paris, le 1er juin 2025.\n"
                "Signé : M. Jean Martin, Gérant de SARL Fictive"
            ),
            "stamp_text": "REÇU\n01/06/2025",
            "stamp_color": (30, 100, 180),
        },
        {
            "file": "stamp_02_cachet",
            "body_ref": (
                "ATTESTATION DE CONFORMITÉ\n\n"
                "Je soussigné, M. Paul Lefebvre, Expert-comptable (fictif),\n"
                "certifie que les comptes annuels de la SAS Garantie FR\n"
                "pour l'exercice clos le 31 décembre 2024 ont été établis\n"
                "conformément aux règles et principes comptables en vigueur.\n\n"
                "Lyon, le 15 mai 2025"
            ),
            "stamp_text": "CACHET\nEXPERT-COMPTABLE\nPAUL LEFEBVRE\n(FICTIF)",
            "stamp_color": (30, 130, 60),
        },
    ]

    font_body  = _get_font(40)
    font_stamp = _get_font(56, bold=True)

    for s in samples:
        img, draw = _new_page()
        y = MARGIN
        y = _wrap_draw(draw, s["body_ref"], MARGIN, y, font_body)

        # Draw a rotated-rectangle stamp overlay
        # PIL doesn't support rotated rectangles easily, so we composite a
        # separate image and paste it at an angle.
        stamp_w = int(2.2 * DPI)
        stamp_h = int(1.4 * DPI)
        stamp_img = Image.new("RGBA", (stamp_w, stamp_h), (255, 255, 255, 0))
        stamp_draw = ImageDraw.Draw(stamp_img)

        stamp_color_rgb = s["stamp_color"]
        border_w = 14
        stamp_draw.rectangle(
            [border_w, border_w, stamp_w - border_w, stamp_h - border_w],
            outline=stamp_color_rgb + (200,),
            width=border_w,
        )
        stamp_draw.rectangle(
            [border_w + 10, border_w + 10, stamp_w - border_w - 10, stamp_h - border_w - 10],
            outline=stamp_color_rgb + (100,),
            width=4,
        )
        stamp_lines = s["stamp_text"].split("\n")
        line_h_stamp = font_stamp.size + 10
        total_h = len(stamp_lines) * line_h_stamp
        sy = (stamp_h - total_h) // 2
        for line in stamp_lines:
            bbox = stamp_draw.textbbox((0, 0), line, font=font_stamp)
            lw = bbox[2] - bbox[0]
            sx = (stamp_w - lw) // 2
            stamp_draw.text((sx, sy), line, font=font_stamp, fill=stamp_color_rgb + (180,))
            sy += line_h_stamp

        rotated = stamp_img.rotate(15, expand=True)
        # Place in lower-right quadrant
        rx = A4_W - MARGIN - rotated.width - int(0.2 * DPI)
        ry = A4_H - MARGIN - rotated.height - int(0.5 * DPI)
        img.paste(rotated, (rx, ry), rotated)

        path = out_dir / f"{s['file']}.png"
        img.save(str(path), dpi=(DPI, DPI))
        ref_path = out_dir / f"{s['file']}.ref.txt"
        # Reference is the body text only (stamp overlay text is secondary)
        ref_text = s["body_ref"] + "\n\n[STAMP: " + s["stamp_text"].replace("\n", " ") + "]"
        ref_path.write_text(ref_text, encoding="utf-8")
        print(f"  written: {path.name}")


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    base = SCRIPT_DIR
    dirs = {
        "printed":     base / "printed",
        "handwritten": base / "handwritten",
        "tables":      base / "tables",
        "stamps":      base / "stamps",
    }
    for d in dirs.values():
        d.mkdir(parents=True, exist_ok=True)

    print("=== Generating printed/ ===")
    _make_printed(dirs["printed"])

    print("=== Generating handwritten/ ===")
    _make_handwritten(dirs["handwritten"])

    print("=== Generating tables/ ===")
    _make_tables(dirs["tables"])

    print("=== Generating stamps/ ===")
    _make_stamps(dirs["stamps"])

    print("\nDone. All fixtures written.")
    total = sum(1 for d in dirs.values() for _ in d.glob("*.png"))
    print(f"Total PNG files: {total}")


if __name__ == "__main__":
    main()
