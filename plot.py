import sys
import json
from collections import defaultdict

pastels_hex_color_theme = [
    "#FFDAB9",  # Peach Puff      (approx. hue ~28°)
    # "#FFDEAD",  # Navajo White    (approx. hue ~36°)
    # "#F5DEB3",  # Wheat           (approx. hue ~39°)
    # "#FFFACD",  # Lemon Chiffon   (approx. hue ~54°)
    # "#F0E68C",  # Khaki           (approx. hue ~54°)
    "#FAFAD2",  # Light Goldenrod (approx. hue ~60°)
    "#D3FFCE",  # Light Green     (approx. hue ~110°)
    # "#E0FFFF",  # Light Cyan      (approx. hue ~180°)
    "#E6E6FA",  # Lavender        (approx. hue ~240°)
    # "#D8BFD8",  # Thistle         (approx. hue ~300°)
    # "#FFC0CB",  # Pink            (approx. hue ~350°)
    # "#FFB6C1",  # Light Pink      (approx. hue ~351°)
]


def build_clusters(nodes):
    clusters = defaultdict(list)
    for node in nodes:
        parts = node.split("/")
        cluster_path = "/".join(parts[:-1]) if len(parts) > 1 else ""
        clusters[cluster_path].append(node)
    return clusters


def get_cluster(node):
    return "/".join(node.split("/")[:-1]) if "/" in node else ""


def generate_color(index: int):
    """Generate a pastel color based on the cluster name."""
    # Convert the name to a consistent hash value

    return pastels_hex_color_theme[index]


def generate_dot(data):
    nodes = data["nodes"]
    clusters = build_clusters(nodes)

    dot = [
        "digraph G {",
        "    compound=true;",
        "    rankdir=LR;",
        '    node [style=filled, fillcolor="#ffffff", shape=box];',
    ]

    def create_subgraphs(cluster_path, parents=None):
        parents = parents or []
        indent = len(parents) + 1

        sub_indent = "    " * indent
        cluster_nodes = clusters.get(cluster_path, [])

        if cluster_path:
            cluster_label = cluster_path[0 if not parents else len(parents[-1]) :]
            print(cluster_path, cluster_nodes, file=sys.stderr)
            color = pastels_hex_color_theme[(indent - 1) % len(pastels_hex_color_theme)]
            dot.append(f'{sub_indent}subgraph "cluster_{cluster_path}" {{')
            dot.append(f'{sub_indent}    label="{cluster_label}";')
            dot.append(f"{sub_indent}    fontsize=20;")
            dot.append(f"{sub_indent}    style=filled;")
            dot.append(f'{sub_indent}    color="{color}";')
            dot.append(f'{sub_indent}    fillcolor="{color}";')

        # Add nodes
        for node in cluster_nodes:
            label = node.split("/")[-1]
            dot.append(f'{sub_indent}    "{node}" [label="{label}"];')

        # Nested clusters
        nested_clusters = {
            k
            for k in clusters
            if k.startswith(cluster_path + "/") and k != cluster_path
        }
        for nested in sorted(nested_clusters):
            create_subgraphs(nested, parents + [cluster_path + "/"])

        if cluster_path:
            dot.append(f"{sub_indent}}}")

    for root_cluster in clusters:
        create_subgraphs(root_cluster)

    # Track seen edges to avoid duplicates
    seen_edges = set()

    # Add edges with lhead/ltail for cross-cluster connections
    for from_node, to_node in data["edges"]:
        from_cluster = get_cluster(from_node)
        to_cluster = get_cluster(to_node)

        edge_attrs = []
        if from_cluster != to_cluster:
            if to_cluster:
                edge_attrs.append(f'lhead="cluster_{to_cluster}"')
            if from_cluster:
                edge_attrs.append(f'ltail="cluster_{from_cluster}"')

        edge_key = ", ".join(edge_attrs)

        # Only add unique edges
        if edge_key not in seen_edges or edge_key == "":
            seen_edges.add(edge_key)
            dot.append(f'    "{from_node}" -> "{to_node}" [{edge_key}];')

    dot.append("}")
    return "\n".join(dot)


def main():
    data = json.load(sys.stdin)
    dot_content = generate_dot(data)
    print(dot_content)


if __name__ == "__main__":
    main()
