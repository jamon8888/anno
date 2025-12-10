#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""
Generate a searchable HTML catalog from datasets_generated.jsonl.

Usage:
    uv run scripts/generate_catalog_html.py
    # or
    python3 scripts/generate_catalog_html.py
"""

import json
from pathlib import Path
from collections import Counter
import html

def main():
    project_root = Path(__file__).parent.parent
    jsonl_path = project_root / "datasets_generated.jsonl"
    output_path = project_root / "docs" / "dataset_catalog.html"
    
    # Ensure docs directory exists
    output_path.parent.mkdir(exist_ok=True)
    
    # Load datasets from JSONL
    datasets = []
    with open(jsonl_path, "r") as f:
        for line in f:
            line = line.strip()
            if line:
                datasets.append(json.loads(line))
    
    # Compute statistics
    categories = Counter(d.get("categories", ["uncategorized"])[0] if d.get("categories") else "uncategorized" for d in datasets)
    languages = Counter(d.get("language", "unknown") for d in datasets)
    formats = Counter(d.get("format", "unknown") for d in datasets)
    
    # Generate HTML
    html_content = generate_html(datasets, categories, languages, formats)
    
    # Write output
    with open(output_path, "w") as f:
        f.write(html_content)
    
    print(f"Generated catalog: {output_path}")
    print(f"  {len(datasets)} datasets")
    print(f"  {len(categories)} categories")
    print(f"  {len(languages)} languages")


def generate_html(datasets, categories, languages, formats):
    # Generate category options
    category_options = "\n".join(
        f'<option value="{html.escape(cat)}">{html.escape(cat)} ({count})</option>'
        for cat, count in sorted(categories.items())
    )
    
    # Generate language options
    language_options = "\n".join(
        f'<option value="{html.escape(lang)}">{html.escape(lang)} ({count})</option>'
        for lang, count in sorted(languages.items())
    )
    
    # Generate dataset rows
    dataset_rows = []
    for d in sorted(datasets, key=lambda x: x.get("name", "")):
        name = html.escape(d.get("name", ""))
        desc = html.escape(d.get("description", "")[:100])
        cat = html.escape((d.get("categories") or [""])[0])
        lang = html.escape(d.get("language", ""))
        fmt = html.escape(d.get("format", ""))
        url = d.get("url", "")
        entity_types = ", ".join(d.get("entity_types", [])[:5])
        if len(d.get("entity_types", [])) > 5:
            entity_types += "..."
        
        url_cell = f'<a href="{html.escape(url)}" target="_blank">Link</a>' if url else "N/A"
        
        dataset_rows.append(f'''
        <tr data-category="{cat}" data-language="{lang}" data-format="{fmt}">
            <td class="name">{name}</td>
            <td class="desc">{desc}</td>
            <td>{cat}</td>
            <td>{lang}</td>
            <td>{fmt}</td>
            <td>{html.escape(entity_types)}</td>
            <td>{url_cell}</td>
        </tr>''')
    
    rows_html = "\n".join(dataset_rows)
    
    return f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Anno Dataset Catalog</title>
    <style>
        :root {{
            --bg: #1a1a2e;
            --surface: #16213e;
            --primary: #e94560;
            --secondary: #0f3460;
            --text: #eaeaea;
            --muted: #8892b0;
        }}
        
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        
        body {{
            font-family: 'Segoe UI', system-ui, sans-serif;
            background: var(--bg);
            color: var(--text);
            line-height: 1.6;
            padding: 2rem;
        }}
        
        h1 {{
            color: var(--primary);
            margin-bottom: 0.5rem;
            font-size: 2.5rem;
        }}
        
        .subtitle {{
            color: var(--muted);
            margin-bottom: 2rem;
        }}
        
        .filters {{
            display: flex;
            gap: 1rem;
            flex-wrap: wrap;
            margin-bottom: 1.5rem;
            background: var(--surface);
            padding: 1rem;
            border-radius: 8px;
        }}
        
        .filter-group {{
            display: flex;
            flex-direction: column;
            gap: 0.25rem;
        }}
        
        .filter-group label {{
            font-size: 0.8rem;
            color: var(--muted);
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }}
        
        input, select {{
            padding: 0.5rem 1rem;
            border: 1px solid var(--secondary);
            border-radius: 4px;
            background: var(--bg);
            color: var(--text);
            font-size: 1rem;
        }}
        
        input:focus, select:focus {{
            outline: none;
            border-color: var(--primary);
        }}
        
        #search {{
            min-width: 300px;
        }}
        
        .stats {{
            display: flex;
            gap: 2rem;
            margin-bottom: 1.5rem;
            color: var(--muted);
        }}
        
        .stat {{
            display: flex;
            align-items: baseline;
            gap: 0.5rem;
        }}
        
        .stat-value {{
            font-size: 1.5rem;
            font-weight: bold;
            color: var(--primary);
        }}
        
        table {{
            width: 100%;
            border-collapse: collapse;
            background: var(--surface);
            border-radius: 8px;
            overflow: hidden;
        }}
        
        th, td {{
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid var(--secondary);
        }}
        
        th {{
            background: var(--secondary);
            color: var(--text);
            font-weight: 600;
            text-transform: uppercase;
            font-size: 0.8rem;
            letter-spacing: 0.05em;
        }}
        
        tr:hover {{
            background: rgba(233, 69, 96, 0.1);
        }}
        
        tr.hidden {{
            display: none;
        }}
        
        .name {{
            font-weight: 600;
            color: var(--primary);
        }}
        
        .desc {{
            color: var(--muted);
            font-size: 0.9rem;
        }}
        
        a {{
            color: var(--primary);
            text-decoration: none;
        }}
        
        a:hover {{
            text-decoration: underline;
        }}
        
        @media (max-width: 768px) {{
            body {{
                padding: 1rem;
            }}
            
            .filters {{
                flex-direction: column;
            }}
            
            #search {{
                min-width: auto;
                width: 100%;
            }}
            
            th, td {{
                padding: 0.5rem;
                font-size: 0.85rem;
            }}
        }}
    </style>
</head>
<body>
    <h1>Anno Dataset Catalog</h1>
    <p class="subtitle">Searchable registry of {len(datasets)} NLP datasets for entity recognition, coreference, and relation extraction</p>
    
    <div class="filters">
        <div class="filter-group">
            <label for="search">Search</label>
            <input type="text" id="search" placeholder="Search by name or description...">
        </div>
        <div class="filter-group">
            <label for="category">Category</label>
            <select id="category">
                <option value="">All Categories</option>
                {category_options}
            </select>
        </div>
        <div class="filter-group">
            <label for="language">Language</label>
            <select id="language">
                <option value="">All Languages</option>
                {language_options}
            </select>
        </div>
    </div>
    
    <div class="stats">
        <div class="stat">
            <span class="stat-value" id="visible-count">{len(datasets)}</span>
            <span>datasets shown</span>
        </div>
        <div class="stat">
            <span class="stat-value">{len(categories)}</span>
            <span>categories</span>
        </div>
        <div class="stat">
            <span class="stat-value">{len(languages)}</span>
            <span>languages</span>
        </div>
    </div>
    
    <table>
        <thead>
            <tr>
                <th>Name</th>
                <th>Description</th>
                <th>Category</th>
                <th>Language</th>
                <th>Format</th>
                <th>Entity Types</th>
                <th>URL</th>
            </tr>
        </thead>
        <tbody id="datasets">
            {rows_html}
        </tbody>
    </table>
    
    <script>
        const searchInput = document.getElementById('search');
        const categorySelect = document.getElementById('category');
        const languageSelect = document.getElementById('language');
        const tbody = document.getElementById('datasets');
        const visibleCount = document.getElementById('visible-count');
        
        function filterTable() {{
            const search = searchInput.value.toLowerCase();
            const category = categorySelect.value;
            const language = languageSelect.value;
            
            let count = 0;
            for (const row of tbody.querySelectorAll('tr')) {{
                const name = row.querySelector('.name')?.textContent.toLowerCase() || '';
                const desc = row.querySelector('.desc')?.textContent.toLowerCase() || '';
                const rowCategory = row.dataset.category || '';
                const rowLanguage = row.dataset.language || '';
                
                const matchesSearch = !search || name.includes(search) || desc.includes(search);
                const matchesCategory = !category || rowCategory === category;
                const matchesLanguage = !language || rowLanguage === language;
                
                if (matchesSearch && matchesCategory && matchesLanguage) {{
                    row.classList.remove('hidden');
                    count++;
                }} else {{
                    row.classList.add('hidden');
                }}
            }}
            
            visibleCount.textContent = count;
        }}
        
        searchInput.addEventListener('input', filterTable);
        categorySelect.addEventListener('change', filterTable);
        languageSelect.addEventListener('change', filterTable);
    </script>
</body>
</html>'''


if __name__ == "__main__":
    main()

