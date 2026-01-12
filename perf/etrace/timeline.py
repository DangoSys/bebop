#!/usr/bin/env python3
"""
Event Trace Timeline Visualizer
Generates an interactive HTML timeline showing events for each module over time.
"""

import json
import sys
from collections import defaultdict
from pathlib import Path


def load_trace(filepath):
    """Load trace file in JSON Lines format."""
    messages = []
    with open(filepath, 'r') as f:
        for line in f:
            line = line.strip()
            if line:
                msg = json.loads(line)
                # Skip messages with null time
                if msg.get('time') is not None:
                    messages.append(msg)
    return messages


def prepare_timeline_data(messages):
    """Prepare data for timeline visualization."""
    # Group events by module
    module_events = defaultdict(list)

    # Track all modules
    all_modules = set()

    for msg in messages:
        source = msg['source']
        target = msg['target']
        time = msg.get('time', 0)

        all_modules.add(source)
        all_modules.add(target)

        # Add event for source module (sending)
        module_events[source].append({
            'time': time,
            'type': 'send',
            'target': target,
            'port': msg['source_port']
        })

        # Add event for target module (receiving)
        module_events[target].append({
            'time': time,
            'type': 'receive',
            'source': source,
            'port': msg['target_port']
        })

    # Sort events by time for each module
    for module in module_events:
        module_events[module].sort(key=lambda x: x['time'])

    return dict(module_events), sorted(all_modules)


def generate_html(module_events, all_modules, output_path):
    """Generate interactive HTML timeline visualization."""

    # Prepare timeline items
    items = []
    groups = []

    # Create groups (one per module)
    for idx, module in enumerate(all_modules):
        groups.append({
            'id': idx,
            'content': module
        })

    # Create timeline items
    module_to_group = {module: idx for idx, module in enumerate(all_modules)}

    item_id = 0
    for module, events in module_events.items():
        group_id = module_to_group[module]

        for event in events:
            time = event['time']
            event_type = event['type']

            if event_type == 'send':
                content = f"→ {event['target']}"
                color = '#007bff'
                title = f"Send to {event['target']}<br>Port: {event['port']}<br>Time: {time:.1f}"
            else:
                content = f"← {event['source']}"
                color = '#28a745'
                title = f"Receive from {event['source']}<br>Port: {event['port']}<br>Time: {time:.1f}"

            items.append({
                'id': item_id,
                'group': group_id,
                'start': time,
                'content': content,
                'title': title,
                'type': 'point',
                'style': f'background-color: {color}; border-color: {color};'
            })
            item_id += 1

    # Calculate statistics
    total_events = sum(len(events) for events in module_events.values())
    time_min = min(e['time'] for events in module_events.values() for e in events) if items else 0
    time_max = max(e['time'] for events in module_events.values() for e in events) if items else 0

    # Generate HTML
    html_content = f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Event Timeline - Bebop Simulator</title>
    <script src="https://unpkg.com/vis-timeline@7.7.3/standalone/umd/vis-timeline-graph2d.min.js"></script>
    <link href="https://unpkg.com/vis-timeline@7.7.3/styles/vis-timeline-graph2d.min.css" rel="stylesheet" type="text/css" />
    <style>
        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            margin: 0;
            padding: 20px;
            background: #f5f5f5;
        }}

        #header {{
            background: white;
            padding: 20px;
            margin-bottom: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}

        h1 {{
            margin: 0 0 10px 0;
            color: #333;
        }}

        .stats {{
            display: flex;
            gap: 20px;
            margin-top: 15px;
        }}

        .stat-box {{
            background: #f8f9fa;
            padding: 10px 15px;
            border-radius: 4px;
            border-left: 4px solid #28a745;
        }}

        .stat-label {{
            font-size: 12px;
            color: #666;
            text-transform: uppercase;
        }}

        .stat-value {{
            font-size: 24px;
            font-weight: bold;
            color: #333;
        }}

        #visualization {{
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            height: 600px;
        }}

        #controls {{
            background: white;
            padding: 15px;
            margin-top: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}

        .control-group {{
            margin: 10px 0;
        }}

        .control-group label {{
            display: inline-block;
            width: 150px;
            font-weight: bold;
        }}

        button {{
            background: #007bff;
            color: white;
            border: none;
            padding: 8px 16px;
            border-radius: 4px;
            cursor: pointer;
            margin-right: 10px;
        }}

        button:hover {{
            background: #0056b3;
        }}

        #legend {{
            background: white;
            padding: 15px;
            margin-top: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}

        .legend-item {{
            display: inline-block;
            margin-right: 20px;
            font-size: 14px;
        }}

        .legend-color {{
            display: inline-block;
            width: 12px;
            height: 12px;
            border-radius: 50%;
            margin-right: 5px;
        }}
    </style>
</head>
<body>
    <div id="header">
        <h1>⏱️ Event Timeline Visualization</h1>
        <div class="stats">
            <div class="stat-box">
                <div class="stat-label">Total Events</div>
                <div class="stat-value">{total_events:,}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Modules</div>
                <div class="stat-value">{len(all_modules)}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Time Range</div>
                <div class="stat-value">{time_min:.1f} - {time_max:.1f}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Duration</div>
                <div class="stat-value">{time_max - time_min:.1f}</div>
            </div>
        </div>
    </div>

    <div id="visualization"></div>

    <div id="controls">
        <h3>🎮 Controls</h3>
        <div class="control-group">
            <button onclick="zoomIn()">🔍 Zoom In</button>
            <button onclick="zoomOut()">🔍 Zoom Out</button>
            <button onclick="fitAll()">📏 Fit All</button>
            <button onclick="moveToStart()">⏮️ Start</button>
            <button onclick="moveToEnd()">⏭️ End</button>
        </div>
    </div>

    <div id="legend">
        <h3>📖 Legend</h3>
        <div class="legend-item">
            <span class="legend-color" style="background: #007bff;"></span>
            Send Event (→)
        </div>
        <div class="legend-item">
            <span class="legend-color" style="background: #28a745;"></span>
            Receive Event (←)
        </div>
    </div>

    <script type="text/javascript">
        // Timeline data
        var items = new vis.DataSet({json.dumps(items)});
        var groups = new vis.DataSet({json.dumps(groups)});

        // Configuration
        var options = {{
            stack: false,
            horizontalScroll: true,
            zoomKey: 'ctrlKey',
            maxHeight: 600,
            minHeight: 600,
            margin: {{
                item: 10,
                axis: 5
            }},
            orientation: 'top',
            showCurrentTime: false,
            zoomMin: 10,
            zoomMax: 1000000000000
        }};

        // Create timeline
        var container = document.getElementById('visualization');
        var timeline = new vis.Timeline(container, items, groups, options);

        // Control functions
        function zoomIn() {{
            timeline.zoomIn(0.5);
        }}

        function zoomOut() {{
            timeline.zoomOut(0.5);
        }}

        function fitAll() {{
            timeline.fit();
        }}

        function moveToStart() {{
            timeline.moveTo({time_min});
        }}

        function moveToEnd() {{
            timeline.moveTo({time_max});
        }}

        // Event listeners
        timeline.on('select', function(properties) {{
            if (properties.items.length > 0) {{
                var item = items.get(properties.items[0]);
                console.log('Selected:', item);
            }}
        }});
    </script>
</body>
</html>
"""

    with open(output_path, 'w') as f:
        f.write(html_content)

    print(f"✅ Timeline visualization generated: {output_path}")


def main():
    if len(sys.argv) < 2:
        print("Usage: python timeline.py <trace.jsonl> [output.html]")
        print("Example: python timeline.py trace.jsonl timeline.html")
        sys.exit(1)

    trace_file = sys.argv[1]
    output_file = sys.argv[2] if len(sys.argv) > 2 else "timeline.html"

    print(f"📖 Loading trace file: {trace_file}")
    messages = load_trace(trace_file)
    print(f"📊 Loaded {len(messages):,} messages")

    print("🔍 Preparing timeline data...")
    module_events, all_modules = prepare_timeline_data(messages)

    print("🎨 Generating HTML visualization...")
    generate_html(module_events, all_modules, output_file)

    print(f"\n📈 Statistics:")
    print(f"  - Modules: {len(all_modules)}")
    print(f"  - Events: {sum(len(e) for e in module_events.values()):,}")
    print(f"\n🌐 Open {output_file} in your browser to view the visualization")


if __name__ == '__main__':
    main()
