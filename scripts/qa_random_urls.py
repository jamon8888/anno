#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""
QA random URL testing for anno extract.

Picks random URLs from a pool of multilingual sources (Wikipedia, Wikimedia,
RSS feeds, etc.) and runs `anno extract --url` on each, producing a markdown
report with entity counts, timing, and error details.

Usage:
    uv run scripts/qa_random_urls.py [OPTIONS]

Examples:
    uv run scripts/qa_random_urls.py --count 3 --no-rss --seed 42
    uv run scripts/qa_random_urls.py --count 10 --backends stacked,bert-onnx
    uv run scripts/qa_random_urls.py --category wikipedia --lang ja,zh,ar
"""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional
from urllib.error import HTTPError, URLError

# ---------------------------------------------------------------------------
# Source pool
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class Source:
    name: str
    url: str
    category: str  # wikipedia, wikimedia, rss, other
    lang: str      # ISO 639-1 or "multi"
    script: str    # Latin, Cyrillic, CJK, Arabic, Devanagari, etc.


# Wikipedia Special:Random -- 45 languages
WIKIPEDIA_SOURCES = [
    Source("Wikipedia (en)", "https://en.wikipedia.org/wiki/Special:Random", "wikipedia", "en", "Latin"),
    Source("Wikipedia (de)", "https://de.wikipedia.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikipedia", "de", "Latin"),
    Source("Wikipedia (fr)", "https://fr.wikipedia.org/wiki/Sp%C3%A9cial:Page_au_hasard", "wikipedia", "fr", "Latin"),
    Source("Wikipedia (es)", "https://es.wikipedia.org/wiki/Especial:Aleatoria", "wikipedia", "es", "Latin"),
    Source("Wikipedia (it)", "https://it.wikipedia.org/wiki/Speciale:PaginaCasuale", "wikipedia", "it", "Latin"),
    Source("Wikipedia (pt)", "https://pt.wikipedia.org/wiki/Especial:Aleat%C3%B3ria", "wikipedia", "pt", "Latin"),
    Source("Wikipedia (nl)", "https://nl.wikipedia.org/wiki/Speciaal:Willekeurig", "wikipedia", "nl", "Latin"),
    Source("Wikipedia (pl)", "https://pl.wikipedia.org/wiki/Specjalna:Losowa_strona", "wikipedia", "pl", "Latin"),
    Source("Wikipedia (sv)", "https://sv.wikipedia.org/wiki/Special:Slumpsida", "wikipedia", "sv", "Latin"),
    Source("Wikipedia (da)", "https://da.wikipedia.org/wiki/Speciel:Tilf%C3%A6ldig_side", "wikipedia", "da", "Latin"),
    Source("Wikipedia (no)", "https://no.wikipedia.org/wiki/Spesial:Tilfeldig_side", "wikipedia", "no", "Latin"),
    Source("Wikipedia (fi)", "https://fi.wikipedia.org/wiki/Toiminnot:Satunnainen_sivu", "wikipedia", "fi", "Latin"),
    Source("Wikipedia (cs)", "https://cs.wikipedia.org/wiki/Speci%C3%A1ln%C3%AD:N%C3%A1hodn%C3%A1_str%C3%A1nka", "wikipedia", "cs", "Latin"),
    Source("Wikipedia (ro)", "https://ro.wikipedia.org/wiki/Special:Aleatoriu", "wikipedia", "ro", "Latin"),
    Source("Wikipedia (hu)", "https://hu.wikipedia.org/wiki/Speci%C3%A1lis:Lap_tal%C3%A1lomra", "wikipedia", "hu", "Latin"),
    Source("Wikipedia (ca)", "https://ca.wikipedia.org/wiki/Especial:P%C3%A0gina_a_l%27atzar", "wikipedia", "ca", "Latin"),
    Source("Wikipedia (tr)", "https://tr.wikipedia.org/wiki/%C3%96zel:Rastgele", "wikipedia", "tr", "Latin"),
    Source("Wikipedia (id)", "https://id.wikipedia.org/wiki/Istimewa:Halaman_sembarang", "wikipedia", "id", "Latin"),
    Source("Wikipedia (ms)", "https://ms.wikipedia.org/wiki/Khas:Rawak", "wikipedia", "ms", "Latin"),
    Source("Wikipedia (vi)", "https://vi.wikipedia.org/wiki/%C4%90%E1%BA%B7c_bi%E1%BB%87t:Ng%E1%BA%ABu_nhi%C3%AAn", "wikipedia", "vi", "Latin"),
    Source("Wikipedia (tl)", "https://tl.wikipedia.org/wiki/Natatangi:Alinmang_pahina", "wikipedia", "tl", "Latin"),
    Source("Wikipedia (sw)", "https://sw.wikipedia.org/wiki/Maalum:UkurasaWowote", "wikipedia", "sw", "Latin"),
    Source("Wikipedia (eu)", "https://eu.wikipedia.org/wiki/Berezi:Ausaz", "wikipedia", "eu", "Latin"),
    Source("Wikipedia (ru)", "https://ru.wikipedia.org/wiki/%D0%A1%D0%BB%D1%83%D0%B6%D0%B5%D0%B1%D0%BD%D0%B0%D1%8F:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0%D1%8F_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikipedia", "ru", "Cyrillic"),
    Source("Wikipedia (uk)", "https://uk.wikipedia.org/wiki/%D0%A1%D0%BF%D0%B5%D1%86%D1%96%D0%B0%D0%BB%D1%8C%D0%BD%D0%B0:%D0%92%D0%B8%D0%BF%D0%B0%D0%B4%D0%BA%D0%BE%D0%B2%D0%B0_%D1%81%D1%82%D0%BE%D1%80%D1%96%D0%BD%D0%BA%D0%B0", "wikipedia", "uk", "Cyrillic"),
    Source("Wikipedia (sr)", "https://sr.wikipedia.org/wiki/%D0%9F%D0%BE%D1%81%D0%B5%D0%B1%D0%BD%D0%BE:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D1%98%D0%BD%D0%B0_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B0", "wikipedia", "sr", "Cyrillic"),
    Source("Wikipedia (bg)", "https://bg.wikipedia.org/wiki/%D0%A1%D0%BF%D0%B5%D1%86%D0%B8%D0%B0%D0%BB%D0%BD%D0%B8:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikipedia", "bg", "Cyrillic"),
    Source("Wikipedia (ja)", "https://ja.wikipedia.org/wiki/%E7%89%B9%E5%88%A5:%E3%81%8A%E3%81%BE%E3%81%8B%E3%81%9B%E8%A1%A8%E7%A4%BA", "wikipedia", "ja", "CJK"),
    Source("Wikipedia (zh)", "https://zh.wikipedia.org/wiki/Special:Random", "wikipedia", "zh", "CJK"),
    Source("Wikipedia (ko)", "https://ko.wikipedia.org/wiki/%ED%8A%B9%EC%88%98:%EC%9E%84%EC%9D%98_%EB%AC%B8%EC%84%9C", "wikipedia", "ko", "CJK"),
    Source("Wikipedia (ar)", "https://ar.wikipedia.org/wiki/%D8%AE%D8%A7%D8%B5:%D8%B9%D8%B4%D9%88%D8%A7%D8%A6%D9%8A", "wikipedia", "ar", "Arabic"),
    Source("Wikipedia (fa)", "https://fa.wikipedia.org/wiki/%D9%88%DB%8C%DA%98%D9%87:%D8%B5%D9%81%D8%AD%D9%87%D9%94_%D8%AA%D8%B5%D8%A7%D8%AF%D9%81%DB%8C", "wikipedia", "fa", "Arabic"),
    Source("Wikipedia (ur)", "https://ur.wikipedia.org/wiki/%D8%AE%D8%A7%D8%B5:%D8%A8%DB%92%D8%AA%D8%B1%D8%AA%DB%8C%D8%A8_%D8%B5%D9%81%D8%AD%DB%81", "wikipedia", "ur", "Arabic"),
    Source("Wikipedia (hi)", "https://hi.wikipedia.org/wiki/%E0%A4%B5%E0%A4%BF%E0%A4%B6%E0%A5%87%E0%A4%B7:%E0%A4%AF%E0%A4%BE%E0%A4%A6%E0%A5%83%E0%A4%9A%E0%A5%8D%E0%A4%9B%E0%A4%BF%E0%A4%95_%E0%A4%AA%E0%A5%83%E0%A4%B7%E0%A5%8D%E0%A4%A0", "wikipedia", "hi", "Devanagari"),
    Source("Wikipedia (mr)", "https://mr.wikipedia.org/wiki/%E0%A4%B5%E0%A4%BF%E0%A4%B6%E0%A5%87%E0%A4%B7:%E0%A4%AF%E0%A4%BE%E0%A4%A6%E0%A5%83%E0%A4%9A%E0%A5%8D%E0%A4%9B%E0%A4%BF%E0%A4%95_%E0%A4%AA%E0%A4%BE%E0%A4%A8", "wikipedia", "mr", "Devanagari"),
    Source("Wikipedia (ne)", "https://ne.wikipedia.org/wiki/%E0%A4%B5%E0%A4%BF%E0%A4%B6%E0%A5%87%E0%A4%B7:%E0%A4%AF%E0%A4%BE%E0%A4%A6%E0%A5%83%E0%A4%9A%E0%A5%8D%E0%A4%9B%E0%A4%BF%E0%A4%95_%E0%A4%AA%E0%A5%83%E0%A4%B7%E0%A5%8D%E0%A4%A0", "wikipedia", "ne", "Devanagari"),
    Source("Wikipedia (th)", "https://th.wikipedia.org/wiki/%E0%B8%9E%E0%B8%B4%E0%B9%80%E0%B8%A8%E0%B8%A9:%E0%B8%AA%E0%B8%B8%E0%B9%88%E0%B8%A1", "wikipedia", "th", "Thai"),
    Source("Wikipedia (ka)", "https://ka.wikipedia.org/wiki/%E1%83%A1%E1%83%9E%E1%83%94%E1%83%AA%E1%83%98%E1%83%90%E1%83%9A%E1%83%A3%E1%83%A0%E1%83%98:%E1%83%A8%E1%83%94%E1%83%9B%E1%83%97%E1%83%AE%E1%83%95%E1%83%94%E1%83%95%E1%83%98%E1%83%97%E1%83%98", "wikipedia", "ka", "Georgian"),
    Source("Wikipedia (hy)", "https://hy.wikipedia.org/wiki/%D5%8D%D5%BA%D5%A1%D5%BD%D5%A1%D6%80%D5%AF%D5%B8%D5%B2:%D5%8A%D5%A1%D5%BF%D5%A1%D5%B0%D5%A1%D5%AF%D5%A1%D5%B6_%D5%A7%D5%BB", "wikipedia", "hy", "Armenian"),
    Source("Wikipedia (el)", "https://el.wikipedia.org/wiki/%CE%95%CE%B9%CE%B4%CE%B9%CE%BA%CF%8C:%CE%A4%CF%85%CF%87%CE%B1%CE%AF%CE%B1", "wikipedia", "el", "Greek"),
    Source("Wikipedia (he)", "https://he.wikipedia.org/wiki/%D7%9E%D7%99%D7%95%D7%97%D7%93:%D7%90%D7%A7%D7%A8%D7%90%D7%99", "wikipedia", "he", "Hebrew"),
    Source("Wikipedia (am)", "https://am.wikipedia.org/wiki/%E1%88%8D%E1%8B%A9:%E1%8B%A8%E1%8B%95%E1%8B%B3%E1%88%8D_%E1%8B%B0%E1%8B%B5%E1%89%A3", "wikipedia", "am", "Ethiopic"),
    Source("Wikipedia (ta)", "https://ta.wikipedia.org/wiki/%E0%AE%9A%E0%AE%BF%E0%AE%B1%E0%AE%AA%E0%AF%8D%E0%AE%AA%E0%AF%81:%E0%AE%9A%E0%AF%86%E0%AE%B4%E0%AF%81%E0%AE%AE%E0%AF%8D%E0%AE%AA%E0%AE%BE%E0%AE%A9_%E0%AE%AA%E0%AE%95%E0%AF%8D%E0%AE%95%E0%AE%AE%E0%AF%8D", "wikipedia", "ta", "Tamil"),
    Source("Wikipedia (bn)", "https://bn.wikipedia.org/wiki/%E0%A6%AC%E0%A6%BF%E0%A6%B6%E0%A7%87%E0%A6%B7:%E0%A6%85%E0%A6%AF%E0%A6%BE%E0%A6%9A%E0%A6%BF%E0%A6%A4", "wikipedia", "bn", "Bengali"),
    Source("Wikipedia (te)", "https://te.wikipedia.org/wiki/%E0%B0%AA%E0%B1%8D%E0%B0%B0%E0%B0%A4%E0%B1%8D%E0%B0%AF%E0%B1%87%E0%B0%95:%E0%B0%AF%E0%B0%BE%E0%B0%A6%E0%B1%83%E0%B0%9A%E0%B1%8D%E0%B0%9B%E0%B0%BF%E0%B0%95%E0%B0%82", "wikipedia", "te", "Telugu"),
]

# Wikimedia siblings -- 35 sources
WIKIMEDIA_SOURCES = [
    # Wiktionary
    Source("Wiktionary (en)", "https://en.wiktionary.org/wiki/Special:Random", "wikimedia", "en", "Latin"),
    Source("Wiktionary (fr)", "https://fr.wiktionary.org/wiki/Sp%C3%A9cial:Page_au_hasard", "wikimedia", "fr", "Latin"),
    Source("Wiktionary (de)", "https://de.wiktionary.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikimedia", "de", "Latin"),
    Source("Wiktionary (ja)", "https://ja.wiktionary.org/wiki/%E7%89%B9%E5%88%A5:%E3%81%8A%E3%81%BE%E3%81%8B%E3%81%9B%E8%A1%A8%E7%A4%BA", "wikimedia", "ja", "CJK"),
    Source("Wiktionary (ru)", "https://ru.wiktionary.org/wiki/%D0%A1%D0%BB%D1%83%D0%B6%D0%B5%D0%B1%D0%BD%D0%B0%D1%8F:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0%D1%8F_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikimedia", "ru", "Cyrillic"),
    Source("Wiktionary (zh)", "https://zh.wiktionary.org/wiki/Special:Random", "wikimedia", "zh", "CJK"),
    Source("Wiktionary (ko)", "https://ko.wiktionary.org/wiki/%ED%8A%B9%EC%88%98:%EC%9E%84%EC%9D%98_%EB%AC%B8%EC%84%9C", "wikimedia", "ko", "CJK"),
    Source("Wiktionary (ar)", "https://ar.wiktionary.org/wiki/%D8%AE%D8%A7%D8%B5:%D8%B9%D8%B4%D9%88%D8%A7%D8%A6%D9%8A", "wikimedia", "ar", "Arabic"),
    # Wikinews
    Source("Wikinews (en)", "https://en.wikinews.org/wiki/Special:Random", "wikimedia", "en", "Latin"),
    Source("Wikinews (fr)", "https://fr.wikinews.org/wiki/Sp%C3%A9cial:Page_au_hasard", "wikimedia", "fr", "Latin"),
    Source("Wikinews (de)", "https://de.wikinews.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikimedia", "de", "Latin"),
    Source("Wikinews (ja)", "https://ja.wikinews.org/wiki/%E7%89%B9%E5%88%A5:%E3%81%8A%E3%81%BE%E3%81%8B%E3%81%9B%E8%A1%A8%E7%A4%BA", "wikimedia", "ja", "CJK"),
    Source("Wikinews (ru)", "https://ru.wikinews.org/wiki/%D0%A1%D0%BB%D1%83%D0%B6%D0%B5%D0%B1%D0%BD%D0%B0%D1%8F:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0%D1%8F_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikimedia", "ru", "Cyrillic"),
    Source("Wikinews (ar)", "https://ar.wikinews.org/wiki/%D8%AE%D8%A7%D8%B5:%D8%B9%D8%B4%D9%88%D8%A7%D8%A6%D9%8A", "wikimedia", "ar", "Arabic"),
    # Wikiquote
    Source("Wikiquote (en)", "https://en.wikiquote.org/wiki/Special:Random", "wikimedia", "en", "Latin"),
    Source("Wikiquote (fr)", "https://fr.wikiquote.org/wiki/Sp%C3%A9cial:Page_au_hasard", "wikimedia", "fr", "Latin"),
    Source("Wikiquote (de)", "https://de.wikiquote.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikimedia", "de", "Latin"),
    Source("Wikiquote (ja)", "https://ja.wikiquote.org/wiki/%E7%89%B9%E5%88%A5:%E3%81%8A%E3%81%BE%E3%81%8B%E3%81%9B%E8%A1%A8%E7%A4%BA", "wikimedia", "ja", "CJK"),
    Source("Wikiquote (ru)", "https://ru.wikiquote.org/wiki/%D0%A1%D0%BB%D1%83%D0%B6%D0%B5%D0%B1%D0%BD%D0%B0%D1%8F:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0%D1%8F_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikimedia", "ru", "Cyrillic"),
    # Wikivoyage
    Source("Wikivoyage (en)", "https://en.wikivoyage.org/wiki/Special:Random", "wikimedia", "en", "Latin"),
    Source("Wikivoyage (de)", "https://de.wikivoyage.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikimedia", "de", "Latin"),
    Source("Wikivoyage (fr)", "https://fr.wikivoyage.org/wiki/Sp%C3%A9cial:Page_au_hasard", "wikimedia", "fr", "Latin"),
    Source("Wikivoyage (zh)", "https://zh.wikivoyage.org/wiki/Special:Random", "wikimedia", "zh", "CJK"),
    Source("Wikivoyage (ru)", "https://ru.wikivoyage.org/wiki/%D0%A1%D0%BB%D1%83%D0%B6%D0%B5%D0%B1%D0%BD%D0%B0%D1%8F:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0%D1%8F_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikimedia", "ru", "Cyrillic"),
    # Wikisource
    Source("Wikisource (en)", "https://en.wikisource.org/wiki/Special:Random", "wikimedia", "en", "Latin"),
    Source("Wikisource (fr)", "https://fr.wikisource.org/wiki/Sp%C3%A9cial:Page_au_hasard", "wikimedia", "fr", "Latin"),
    Source("Wikisource (de)", "https://de.wikisource.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikimedia", "de", "Latin"),
    Source("Wikisource (zh)", "https://zh.wikisource.org/wiki/Special:Random", "wikimedia", "zh", "CJK"),
    Source("Wikisource (ru)", "https://ru.wikisource.org/wiki/%D0%A1%D0%BB%D1%83%D0%B6%D0%B5%D0%B1%D0%BD%D0%B0%D1%8F:%D0%A1%D0%BB%D1%83%D1%87%D0%B0%D0%B9%D0%BD%D0%B0%D1%8F_%D1%81%D1%82%D1%80%D0%B0%D0%BD%D0%B8%D1%86%D0%B0", "wikimedia", "ru", "Cyrillic"),
    Source("Wikisource (ar)", "https://ar.wikisource.org/wiki/%D8%AE%D8%A7%D8%B5:%D8%B9%D8%B4%D9%88%D8%A7%D8%A6%D9%8A", "wikimedia", "ar", "Arabic"),
    Source("Wikisource (ja)", "https://ja.wikisource.org/wiki/%E7%89%B9%E5%88%A5:%E3%81%8A%E3%81%BE%E3%81%8B%E3%81%9B%E8%A1%A8%E7%A4%BA", "wikimedia", "ja", "CJK"),
    # Wikibooks
    Source("Wikibooks (en)", "https://en.wikibooks.org/wiki/Special:Random", "wikimedia", "en", "Latin"),
    Source("Wikibooks (de)", "https://de.wikibooks.org/wiki/Spezial:Zuf%C3%A4llige_Seite", "wikimedia", "de", "Latin"),
    Source("Wikibooks (ja)", "https://ja.wikibooks.org/wiki/%E7%89%B9%E5%88%A5:%E3%81%8A%E3%81%BE%E3%81%8B%E3%81%9B%E8%A1%A8%E7%A4%BA", "wikimedia", "ja", "CJK"),
]

# Non-wiki random endpoints
OTHER_SOURCES = [
    Source("Simple English Wikipedia", "https://simple.wikipedia.org/wiki/Special:Random", "other", "en", "Latin"),
    Source("RationalWiki", "https://rationalwiki.org/wiki/Special:Random", "other", "en", "Latin"),
    Source("Project Gutenberg (random)", "https://www.gutenberg.org/ebooks/search/?sort_order=random", "other", "en", "Latin"),
    Source("OpenLibrary (random)", "https://openlibrary.org/random", "other", "en", "Latin"),
    Source("Wikimedia Commons", "https://commons.wikimedia.org/wiki/Special:Random", "other", "multi", "Latin"),
]

# RSS feed sources -- parsed at runtime
RSS_SOURCES = [
    # English news
    Source("BBC News", "http://feeds.bbci.co.uk/news/rss.xml", "rss", "en", "Latin"),
    Source("BBC Science", "http://feeds.bbci.co.uk/news/science_and_environment/rss.xml", "rss", "en", "Latin"),
    Source("NPR News", "https://feeds.npr.org/1001/rss.xml", "rss", "en", "Latin"),
    Source("Reuters World", "https://www.reutersagency.com/feed/?taxonomy=best-sectors&post_type=best", "rss", "en", "Latin"),
    # Multilingual news
    Source("BBC Arabic", "http://feeds.bbci.co.uk/arabic/rss.xml", "rss", "ar", "Arabic"),
    Source("BBC Russian", "http://feeds.bbci.co.uk/russian/rss.xml", "rss", "ru", "Cyrillic"),
    Source("DW German", "https://rss.dw.com/rdf/rss-de-all", "rss", "de", "Latin"),
    Source("DW English", "https://rss.dw.com/rdf/rss-en-all", "rss", "en", "Latin"),
    Source("France24 French", "https://www.france24.com/fr/rss", "rss", "fr", "Latin"),
    Source("France24 English", "https://www.france24.com/en/rss", "rss", "en", "Latin"),
    Source("France24 Arabic", "https://www.france24.com/ar/rss", "rss", "ar", "Arabic"),
    Source("Al Jazeera English", "https://www.aljazeera.com/xml/rss/all.xml", "rss", "en", "Latin"),
    Source("NHK World", "https://www3.nhk.or.jp/rss/news/cat0.xml", "rss", "ja", "CJK"),
    # Tech
    Source("ArXiv CS.CL", "https://rss.arxiv.org/rss/cs.CL", "rss", "en", "Latin"),
    Source("ArXiv CS.AI", "https://rss.arxiv.org/rss/cs.AI", "rss", "en", "Latin"),
    Source("Hacker News", "https://hnrss.org/frontpage", "rss", "en", "Latin"),
    Source("Lobsters", "https://lobste.rs/rss", "rss", "en", "Latin"),
]

ALL_SOURCES = WIKIPEDIA_SOURCES + WIKIMEDIA_SOURCES + OTHER_SOURCES + RSS_SOURCES


# ---------------------------------------------------------------------------
# RSS helper
# ---------------------------------------------------------------------------

def fetch_random_rss_link(feed_url: str, timeout: int = 15) -> Optional[str]:
    """Fetch an RSS/Atom feed and return one random entry link."""
    try:
        req = urllib.request.Request(feed_url, headers={"User-Agent": "anno-qa/1.0"})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = resp.read()
        root = ET.fromstring(data)

        links: list[str] = []

        # RSS 2.0: channel/item/link
        for item in root.iter("item"):
            link_el = item.find("link")
            if link_el is not None and link_el.text:
                links.append(link_el.text.strip())

        # Atom: entry/link[@href]
        ns = {"atom": "http://www.w3.org/2005/Atom"}
        for entry in root.iter("{http://www.w3.org/2005/Atom}entry"):
            for link_el in entry.findall("atom:link", ns):
                href = link_el.get("href")
                if href:
                    links.append(href.strip())
            # Also try without namespace (some feeds)
            for link_el in entry.findall("link"):
                href = link_el.get("href") or (link_el.text or "").strip()
                if href:
                    links.append(href)

        # RDF: rdf:item/link
        for item in root.iter("{http://purl.org/rss/1.0/}item"):
            link_el = item.find("{http://purl.org/rss/1.0/}link")
            if link_el is not None and link_el.text:
                links.append(link_el.text.strip())

        if not links:
            return None
        return random.choice(links)

    except (HTTPError, URLError, ET.ParseError, TimeoutError, OSError):
        return None


# ---------------------------------------------------------------------------
# Anno runner
# ---------------------------------------------------------------------------

@dataclass
class RunResult:
    source: Source
    url: str
    status: str  # OK, error, timeout
    entity_count: int = 0
    type_counts: dict[str, int] = field(default_factory=dict)
    elapsed_secs: float = 0.0
    error_msg: str = ""
    stderr_tail: str = ""


def resolve_url(source: Source) -> Optional[str]:
    """Resolve the actual URL to test. For RSS sources, fetch the feed first."""
    if source.category == "rss":
        return fetch_random_rss_link(source.url)
    return source.url


def run_anno(
    anno_bin: str,
    url: str,
    backend: str,
    timeout: int,
    verbose: bool,
) -> RunResult:
    """Run anno extract on a URL and parse the result."""
    # placeholder -- filled by caller
    result = RunResult(
        source=Source("", "", "", "", ""),
        url=url,
        status="error",
    )

    cmd = [
        anno_bin, "extract",
        "--url", url,
        "--model", backend,
        "--format", "json",
        "--detect-lang",
    ]

    t0 = time.monotonic()
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        elapsed = time.monotonic() - t0
        result.elapsed_secs = elapsed

        if verbose:
            if proc.stderr:
                print(f"  stderr: {proc.stderr[:500]}", file=sys.stderr)

        if proc.returncode != 0:
            result.status = "error"
            result.error_msg = f"exit code {proc.returncode}"
            result.stderr_tail = (proc.stderr or "")[-500:]
            return result

        # Parse JSON output
        try:
            data = json.loads(proc.stdout)
        except json.JSONDecodeError as e:
            result.status = "error"
            result.error_msg = f"JSON parse error: {e}"
            return result

        entities = data.get("entities", [])
        prov = data.get("provenance", {})

        result.status = "OK"
        result.entity_count = len(entities)

        type_counts: dict[str, int] = {}
        for ent in entities:
            t = ent.get("type", "?")
            type_counts[t] = type_counts.get(t, 0) + 1
        result.type_counts = type_counts

        # Use provenance elapsed_ms if available (more accurate)
        if "elapsed_ms" in prov:
            result.elapsed_secs = prov["elapsed_ms"] / 1000.0

        return result

    except subprocess.TimeoutExpired:
        result.elapsed_secs = time.monotonic() - t0
        result.status = "timeout"
        result.error_msg = f"timeout after {timeout}s"
        return result
    except FileNotFoundError:
        result.status = "error"
        result.error_msg = f"anno binary not found: {anno_bin}"
        return result


# ---------------------------------------------------------------------------
# Report generation
# ---------------------------------------------------------------------------

def generate_report(
    results: list[RunResult],
    seed: Optional[int],
    backends: list[str],
    start_time: datetime,
) -> str:
    lines: list[str] = []
    ok_count = sum(1 for r in results if r.status == "OK")
    err_count = len(results) - ok_count
    all_langs = sorted(set(r.source.lang for r in results))
    all_scripts = sorted(set(r.source.script for r in results))

    total_types: Counter[str] = Counter()
    total_entities = 0
    for r in results:
        total_entities += r.entity_count
        total_types.update(r.type_counts)

    seed_str = str(seed) if seed is not None else "random"
    backend_str = ", ".join(backends)

    lines.append("# QA Random URL Report")
    lines.append(f"Date: {start_time.strftime('%Y-%m-%d %H:%M:%S UTC')} | Seed: {seed_str} | Backend: {backend_str}")
    lines.append("")
    lines.append("## Summary")
    lines.append(f"- URLs tested: {ok_count}/{len(results)}")
    lines.append(f"- Failures: {err_count}" + (f" ({', '.join(r.status for r in results if r.status != 'OK')})" if err_count else ""))
    lines.append(f"- Languages: {', '.join(all_langs)}")
    lines.append(f"- Scripts: {', '.join(all_scripts)}")
    type_summary = ", ".join(f"{t}: {c}" for t, c in total_types.most_common())
    lines.append(f"- Entities found: {total_entities} total ({type_summary})")
    lines.append("")

    # Results table
    lines.append("## Results")
    lines.append("")
    lines.append("| # | Source | URL | Lang | Script | Entities | Time | Status |")
    lines.append("|---|--------|-----|------|--------|----------|------|--------|")

    for i, r in enumerate(results, 1):
        url_display = r.url
        if len(url_display) > 60:
            url_display = url_display[:57] + "..."
        time_str = f"{r.elapsed_secs:.1f}s"
        ent_str = str(r.entity_count) if r.status == "OK" else "-"
        status_icon = "OK" if r.status == "OK" else f"**{r.status}**"
        lines.append(
            f"| {i} | {r.source.name} | {url_display} | {r.source.lang} | {r.source.script} | {ent_str} | {time_str} | {status_icon} |"
        )

    lines.append("")

    # Type breakdown
    if total_types:
        lines.append("## Entity Type Breakdown")
        lines.append("")
        lines.append("| Type | Count | % |")
        lines.append("|------|-------|---|")
        for t, c in total_types.most_common():
            pct = (c / total_entities * 100) if total_entities > 0 else 0
            lines.append(f"| {t} | {c} | {pct:.1f}% |")
        lines.append("")

    # Errors section
    errors = [(i, r) for i, r in enumerate(results, 1) if r.status != "OK"]
    if errors:
        lines.append("## Errors")
        lines.append("")
        for i, r in errors:
            lines.append(f"### [{i}] {r.source.name}")
            lines.append(f"- URL: {r.url}")
            lines.append(f"- Error: {r.error_msg}")
            if r.stderr_tail:
                lines.append(f"- stderr (tail): `{r.stderr_tail[:200]}`")
            lines.append("")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="QA random URL testing for anno extract",
    )
    p.add_argument("--count", "-n", type=int, default=10, help="Number of URLs to test (default: 10)")
    p.add_argument("--backends", "-b", default="stacked", help="Comma-separated backends (default: stacked)")
    p.add_argument("--timeout", type=int, default=60, help="Per-URL timeout in seconds (default: 60)")
    p.add_argument("--output", "-o", help="Report file path (default: reports/qa-random-<timestamp>.md)")
    p.add_argument("--category", "-c", default="all", choices=["wikipedia", "wikimedia", "rss", "other", "all"],
                   help="Filter source category (default: all)")
    p.add_argument("--lang", help="Filter by language codes (comma-separated)")
    p.add_argument("--seed", "-s", type=int, help="Random seed for reproducibility")
    p.add_argument("--no-rss", action="store_true", help="Skip RSS-based sources (faster)")
    p.add_argument("--verbose", "-v", action="store_true", help="Show full anno output")
    p.add_argument("--delay", type=float, default=1.0, help="Delay between requests in seconds (default: 1.0)")
    return p.parse_args()


def main() -> int:
    args = parse_args()
    start_time = datetime.now(timezone.utc)
    anno_bin = os.environ.get("ANNO", "./target/release/anno")
    backends = [b.strip() for b in args.backends.split(",")]

    # Seed
    if args.seed is not None:
        random.seed(args.seed)

    # Filter sources
    pool = list(ALL_SOURCES)
    if args.no_rss:
        pool = [s for s in pool if s.category != "rss"]
    if args.category != "all":
        pool = [s for s in pool if s.category == args.category]
    if args.lang:
        lang_set = {l.strip() for l in args.lang.split(",")}
        pool = [s for s in pool if s.lang in lang_set]

    if not pool:
        print("error: No sources match the given filters.", file=sys.stderr)
        return 1

    # Sample
    count = min(args.count, len(pool))
    selected = random.sample(pool, count)

    print(f"QA random URL test: {count} URLs, backend(s): {', '.join(backends)}, seed: {args.seed}")
    print(f"Source pool: {len(pool)} sources after filters")
    print()

    all_results: list[RunResult] = []

    for i, source in enumerate(selected, 1):
        # Resolve URL (RSS needs feed fetch)
        url = resolve_url(source)
        if url is None:
            print(f"  [{i}/{count}] {source.name}: RSS fetch failed, skipping")
            r = RunResult(source=source, url=source.url, status="error",
                          error_msg="RSS feed fetch failed")
            all_results.append(r)
            continue

        for backend in backends:
            print(f"  [{i}/{count}] {source.name} ({source.lang}/{source.script}) -> {backend}...", end=" ", flush=True)
            result = run_anno(anno_bin, url, backend, args.timeout, args.verbose)
            result.source = source
            result.url = url  # resolved URL

            if result.status == "OK":
                top_types = ", ".join(f"{t}:{c}" for t, c in sorted(result.type_counts.items(), key=lambda x: -x[1])[:3])
                print(f"{result.entity_count} entities ({top_types}) in {result.elapsed_secs:.1f}s")
            else:
                print(f"{result.status}: {result.error_msg}")

            all_results.append(result)

        # Rate limiting
        if i < count and args.delay > 0:
            time.sleep(args.delay)

    # Generate report
    report = generate_report(all_results, args.seed, backends, start_time)

    # Output
    if args.output:
        out_path = Path(args.output)
    else:
        ts = start_time.strftime("%Y%m%d-%H%M%S")
        out_path = Path("reports") / f"qa-random-{ts}.md"

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(report, encoding="utf-8")
    print(f"\nReport written to {out_path}")

    # Summary
    ok_count = sum(1 for r in all_results if r.status == "OK")
    total = len(all_results)
    print(f"Done: {ok_count}/{total} OK")

    return 0 if ok_count == total else 1


if __name__ == "__main__":
    raise SystemExit(main())
