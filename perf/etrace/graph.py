#!/usr/bin/env python3
"""
Event Trace Graph Visualizer
Generates an interactive HTML visualization showing module connections and event statistics.
"""

import json
import sys
from collections import defaultdict, Counter
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


def analyze_trace(messages):
    """Analyze trace to extract graph and statistics."""
    # Module connections: (source, target) -> count
    connections = Counter()

    # Module activity: module -> event count
    module_activity = Counter()

    # Port usage: (module, port) -> count
    port_usage = Counter()

    # Timeline stats
    time_range = (float('inf'), float('-inf'))

    for msg in messages:
        source = msg['source']
        target = msg['target']
        source_port = msg['source_port']
        target_port = msg['target_port']
        time = msg.get('time', 0)

        connections[(source, target)] += 1
        module_activity[source] += 1
        module_activity[target] += 1

        port_usage[(source, source_port)] += 1
        port_usage[(target, target_port)] += 1

        time_range = (min(time_range[0], time), max(time_range[1], time))

    # Handle case with no valid messages
    if time_range[0] == float('inf'):
        time_range = (0, 0)

    return {
        'connections': dict(connections),
        'module_activity': dict(module_activity),
        'port_usage': dict(port_usage),
        'time_range': time_range,
        'total_messages': len(messages)
    }


def generate_html(stats, output_path):
    """Generate interactive HTML visualization."""

    # Prepare data for visualization
    nodes = []
    node_map = {}
    node_id = 0

    for module, count in stats['module_activity'].items():
        nodes.append({
            'id': node_id,
            'label': module,
            'value': count,
            'title': f"{module}<br>Events: {count}"
        })
        node_map[module] = node_id
        node_id += 1

    edges = []
    for (source, target), count in stats['connections'].items():
        if source in node_map and target in node_map:
            edges.append({
                'from': node_map[source],
                'to': node_map[target],
                'value': count,
                'title': f"{source} → {target}<br>Messages: {count}",
                'arrows': 'to'
            })

    # Generate HTML with vis.js
    html_content = f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Event Trace Graph - Bebop Simulator</title>
    <script type="text/javascript" src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
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
            border-left: 4px solid #007bff;
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

        #network {{
            width: 100%;
            height: 600px;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}

        #legend {{
            background: white;
            padding: 15px;
            margin-top: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}

        .legend-item {{
            margin: 5px 0;
            font-size: 14px;
            color: #555;
        }}
    </style>
</head>
<body>
    <div id="header">
        <h1>🔍 Event Trace Graph Visualization</h1>
        <div class="stats">
            <div class="stat-box">
                <div class="stat-label">Total Messages</div>
                <div class="stat-value">{stats['total_messages']:,}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Modules</div>
                <div class="stat-value">{len(stats['module_activity'])}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Connections</div>
                <div class="stat-value">{len(stats['connections'])}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Time Range</div>
                <div class="stat-value">{stats['time_range'][0]:.1f} - {stats['time_range'][1]:.1f}</div>
            </div>
        </div>
    </div>

    <div id="network"></div>

    <div id="legend">
        <h3>📊 Module Activity Ranking</h3>
        <div id="module-list"></div>
    </div>

    <script type="text/javascript">
        // Network data
        var nodes = new vis.DataSet({json.dumps(nodes)});
        var edges = new vis.DataSet({json.dumps(edges)});

        // Create network
        var container = document.getElementById('network');
        var data = {{
            nodes: nodes,
            edges: edges
        }};

        var options = {{
            nodes: {{
                shape: 'dot',
                scaling: {{
                    min: 10,
                    max: 50,
                    label: {{
                        enabled: true,
                        min: 14,
                        max: 24
                    }}
                }},
                font: {{
                    size: 14,
                    face: 'Segoe UI'
                }},
                color: {{
                    background: '#007bff',
                    border: '#0056b3',
                    highlight: {{
                        background: '#0056b3',
                        border: '#003d82'
                    }}
                }}
            }},
            edges: {{
                width: 1,
                color: {{
                    color: '#848484',
                    highlight: '#007bff'
                }},
                arrows: {{
                    to: {{
                        enabled: true,
                        scaleFactor: 0.5
                    }}
                }},
                smooth: {{
                    type: 'continuous',
                    roundness: 0.5
                }},
                scaling: {{
                    min: 1,
                    max: 10
                }}
            }},
            physics: {{
                stabilization: {{
                    enabled: true,
                    iterations: 200
                }},
                barnesHut: {{
                    gravitationalConstant: -8000,
                    centralGravity: 0.3,
                    springLength: 150,
                    springConstant: 0.04
                }}
            }},
            interaction: {{
                hover: true,
                tooltipDelay: 100,
                navigationButtons: true,
                keyboard: true
            }}
        }};

        var network = new vis.Network(container, data, options);

        // Generate module ranking
        var moduleActivity = {json.dumps(sorted(stats['module_activity'].items(), key=lambda x: x[1], reverse=True))};
        var moduleList = document.getElementById('module-list');

        moduleActivity.forEach(function(item, index) {{
            var div = document.createElement('div');
            div.className = 'legend-item';
            div.innerHTML = '<b>' + (index + 1) + '.</b> ' + item[0] + ': <b>' + item[1].toLocaleString() + '</b> events';
            moduleList.appendChild(div);
        }});
    </script>
</body>
</html>
"""

    with open(output_path, 'w') as f:
        f.write(html_content)

    print(f"✅ Graph visualization generated: {output_path}")


def main():
    if len(sys.argv) < 2:
        print("Usage: python graph.py <trace.jsonl> [output.html]")
        print("Example: python graph.py trace.jsonl graph.html")
        sys.exit(1)

    trace_file = sys.argv[1]
    output_file = sys.argv[2] if len(sys.argv) > 2 else "graph.html"

    print(f"📖 Loading trace file: {trace_file}")
    messages = load_trace(trace_file)
    print(f"📊 Loaded {len(messages):,} messages")

    print("🔍 Analyzing trace...")
    stats = analyze_trace(messages)

    print("🎨 Generating HTML visualization...")
    generate_html(stats, output_file)

    print(f"\n📈 Statistics:")
    print(f"  - Modules: {len(stats['module_activity'])}")
    print(f"  - Connections: {len(stats['connections'])}")
    print(f"  - Time range: {stats['time_range'][0]:.1f} - {stats['time_range'][1]:.1f}")
    print(f"\n🌐 Open {output_file} in your browser to view the visualization")


if __name__ == '__main__':
    main()
