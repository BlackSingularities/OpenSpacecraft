# Katalog statków kosmicznych i startów

Dane są **statyczne**: pobiera się je raz, skrypty łączą je w `data/catalog/*.json`, a program
wbudowuje te pliki w siebie. Z sieci program dociąga tylko zdjęcia (z pamięcią podręczną na dysku).

| Źródło | Co daje | Licencja |
|---|---|---|
| [Wikidata](https://www.wikidata.org) (SPARQL) | obiekty z COSPAR: nazwy EN/PL, daty, operator, kraj, rakieta, kosmodrom, masa, zdjęcie, linki do Wikipedii | CC0 |
| [NASA NSSDCA Master Catalog](https://nssdc.gsfc.nasa.gov/nmc/) | pełne opisy, fakty, dziedziny, inne nazwy | domena publiczna |
| [Launch Library 2](https://thespacedevs.com/llapi) (The Space Devs) | historia startów: godzina, rakieta, stanowisko, opis misji, zdjęcie | dane udostępniane z podaniem źródła |
| [CelesTrak SATCAT](https://celestrak.org/satcat/) | właściciel obiektu, NORAD, data deorbitacji | publiczne |

## Odświeżenie danych

```bash
python -I tools/catalog/fetch_wikidata.py     # ~2 min
python -I tools/catalog/fetch_nssdca.py       # ~1 h (strony cache'owane w raw/, można przerwać i wznowić)
python -I tools/catalog/fetch_ll2.py          # ~2 min
curl -o tools/catalog/raw/satcat.csv https://celestrak.org/pub/satcat.csv
python -I tools/catalog/build_catalog.py      # fuzja → data/catalog/
```

Surowe odpowiedzi leżą w `tools/catalog/raw/` (poza gitem). Aby wymusić ponowne pobranie, usuń odpowiedni plik lub katalog.

## Fuzja z misjami atlasu

`build_catalog.py` łączy wpisy katalogu z misjami z `data/missions.json`: po polu `cospar` w misji (ręczne
powiązanie, ma pierwszeństwo), po numerze NORAD, po nazwie i dniu startu, a na końcu po samym dniu startu.
Powiązany statek dostaje pole `m` – w programie karta misji pokazuje wtedy zdjęcie, pełny opis NSSDCA i źródła.

## Tłumaczenia

Teksty źródłowe są po angielsku; polskie tłumaczenia trzyma `data/catalog/pl.json` (klucz → tekst):

- `term:<pojęcie>` – typy, dziedziny, orbity, statusy,
- `sc:<COSPAR>:short` / `sc:<COSPAR>:desc` – krótki i pełny opis statku,
- `ln:<id LL2>:desc` – opis startu.

Edytor w przeglądarce: `python -I tools/translate/server.py` → http://127.0.0.1:8765.
Brak tłumaczenia = program pokazuje angielski oryginał z dopiskiem. Po zmianach przebuduj program.
