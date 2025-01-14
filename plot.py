import sys
import json
from collections import defaultdict
import hashlib


def find_common_prefix(paths):
    split_paths = [path.split("/") for path in paths]
    min_len = min(len(p) for p in split_paths)

    prefix = []
    for i in range(min_len):
        current = split_paths[0][i]
        if all(p[i] == current for p in split_paths):
            prefix.append(current)
        else:
            break
    return "/".join(prefix)


def build_clusters(nodes):
    clusters = defaultdict(list)
    for node in nodes:
        parts = node.split("/")
        cluster_path = "/".join(parts[:-1]) if len(parts) > 1 else ""
        clusters[cluster_path].append(node)
    return clusters


def get_cluster(node):
    return "/".join(node.split("/")[:-1]) if "/" in node else ""


def generate_color(name):
    """Generate a pastel color based on the cluster name."""
    hash_object = hashlib.md5(name.encode())
    hex_color = hash_object.hexdigest()[:6]

    # Soften the color by blending it with white
    r = int(hex_color[0:2], 16)
    g = int(hex_color[2:4], 16)
    b = int(hex_color[4:6], 16)

    # Blend with white for a pastel effect
    r = int((r + 255) / 2)
    g = int((g + 255) / 2)
    b = int((b + 255) / 2)

    return f"#{r:02x}{g:02x}{b:02x}"


def generate_dot(data):
    nodes = data["nodes"]
    clusters = build_clusters(nodes)
    root_cluster = find_common_prefix(nodes)

    dot = [
        "digraph G {",
        "    compound=true;",
        "    rankdir=LR;",
        '    node [style=filled, fillcolor="#ffffff", shape=box];',
    ]

    def create_subgraphs(cluster_path, indent=1):
        sub_indent = "    " * indent
        if cluster_path:
            cluster_name = cluster_path.replace("/", "_")
            color = generate_color(cluster_path)  # Generate color for the cluster
            dot.append(f'{sub_indent}subgraph "cluster_{cluster_name}" {{')
            dot.append(f'{sub_indent}    label="{cluster_path}";')
            dot.append(f"{sub_indent}    style=filled;")
            dot.append(f'{sub_indent}    color="{color}";')
            dot.append(f'{sub_indent}    fillcolor="{color}";')

        # Add nodes
        for node in clusters.get(cluster_path, []):
            dot.append(f'{sub_indent}    "{node}";')

        # Nested clusters
        nested_clusters = {
            k
            for k in clusters
            if k.startswith(cluster_path + "/") and k != cluster_path
        }
        for nested in sorted(nested_clusters):
            create_subgraphs(nested, indent + 1)

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
                edge_attrs.append(f'lhead="cluster_{to_cluster.replace("/", "_")}"')
            if from_cluster:
                edge_attrs.append(f'ltail="cluster_{from_cluster.replace("/", "_")}"')

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
