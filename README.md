# Crawl4AI — Open-source LLM Friendly Web Crawler & Scraper

> Fork communautaire de [unclecode/crawl4ai](https://github.com/unclecode/crawl4ai), dépersonnalisé pour un usage autonome. Le moteur est identique à l'upstream.

Crawl4AI est un crawler/scraper web asynchrone, rapide et prêt pour les LLM : HTML → Markdown propre, extraction structurée (CSS/XPath/LLM), deep crawling, cache, Docker, CLI.

## Quick Start

```bash
pip install -U crawl4ai
crawl4ai-setup      # installe les navigateurs (Playwright)
crawl4ai-doctor     # vérifie l'installation
```

```python
import asyncio
from crawl4ai import AsyncWebCrawler

async def main():
    async with AsyncWebCrawler() as crawler:
        result = await crawler.arun(url="https://example.com")
        print(result.markdown)

if __name__ == "__main__":
    asyncio.run(main())
```

Ou via CLI :

```bash
crwl https://example.com -o markdown
crwl https://example.com --deep-crawl bfs --max-pages 10
```

## Fonctionnalités principales

- **Markdown optimisé LLM** : filtres de contenu (Pruning, BM25), génération GFM, tableaux
- **Extraction structurée** : JSON via CSS/XPath, regex, stratégies LLM, cosine
- **Crawling intelligent** : deep crawl (BFS/DFS/BFF), adaptive crawling, virtual scroll, profils navigateur
- **Échelle** : dispatch asynchrone adaptatif à la mémoire, rate limiting, seeding massif d'URLs
- **Déploiement** : image Docker + API serveur (`deploy/docker/`), docker-compose

## Documentation

- `docs/` — source du site de documentation (mkdocs)
- `ROADMAP` et changelog : voir les releases GitHub de ce dépôt

## Attribution & Licence

Ce projet est un fork de [Crawl4AI](https://github.com/unclecode/crawl4ai) (créé par Unclecode), distribué sous **Apache License 2.0** — voir [LICENSE](LICENSE). Les modifications locales sont limitées à la dépersonnalisation ; le moteur reste celui de l'upstream.