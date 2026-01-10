# Event Trace Visualization

Event trace visualization tools for the Bebop simulator. Analyzes JSON Lines trace files and generates interactive HTML visualizations.

**graph.py**: Generates an interactive network graph showing module connections and event statistics.
**timeline.py**: Creates a timeline visualization displaying events for each module over time.

## Usage

```bash
python graph.py trace.jsonl [output.html]
python timeline.py trace.jsonl [output.html]
```

Both tools read JSON Lines format and produce standalone HTML files viewable in any browser.
