import re
import math
from typing import List, Tuple
import math
from collections import defaultdict
import json
import xml.etree.ElementTree as ET
import re
from svg.path import parse_path

from typing import List, Tuple
from typing import Tuple
Point = tuple[float, float]
LineSegment = tuple[Point, Point]

def parse_svg_paths(svg_content: str) -> List[LineSegment]:
    """Parse SVG <path> d attribute into line segments, approximating Bezier curves."""
    segments = []
    tree = ET.fromstring(svg_content)
    ns = {'svg': 'http://www.w3.org/2000/svg'}
    
    for path_elem in tree.findall('.//svg:path', ns):
        d = path_elem.get('d', '')
        if not d:
            continue
        
        # Simple tokenization for M, L, C, Z
        tokens = re.findall(r'[MLCZmlcz]|-?\d*\.?\d+(?:[eE][+-]?\d+)?', d)
        i = 0
        current_pos = (0.0, 0.0)
        start_pos = (0.0, 0.0)
        
        while i < len(tokens):
            cmd = tokens[i]
            i += 1
            if cmd.upper() == 'M':
                if i + 1 < len(tokens):
                    x = float(tokens[i])
                    y = float(tokens[i+1])
                    current_pos = (x, y)
                    start_pos = current_pos
                    i += 2
            elif cmd.upper() == 'L':
                while i + 1 < len(tokens) and not tokens[i].isalpha():
                    x = float(tokens[i])
                    y = float(tokens[i+1])
                    segments.append((current_pos, (x, y)))
                    current_pos = (x, y)
                    i += 2
            elif cmd.upper() == 'C':
                while i + 5 < len(tokens) and not tokens[i+5].isalpha():
                    x1 = float(tokens[i])
                    y1 = float(tokens[i+1])
                    x2 = float(tokens[i+2])
                    y2 = float(tokens[i+3])
                    x = float(tokens[i+4])
                    y = float(tokens[i+5])
                    # Approximate cubic Bezier with 4 line segments
                    for t in [0.25, 0.5, 0.75, 1.0]:
                        if t == 0:
                            continue
                        px = (1-t)**3 * current_pos[0] + 3*(1-t)**2*t * x1 + 3*(1-t)*t**2 * x2 + t**3 * x
                        py = (1-t)**3 * current_pos[1] + 3*(1-t)**2*t * y1 + 3*(1-t)*t**2 * y2 + t**3 * y
                        segments.append((current_pos, (px, py)))
                        current_pos = (px, py)
                    current_pos = (x, y)
                    i += 6
            elif cmd.upper() == 'Z':
                if current_pos != start_pos:
                    segments.append((current_pos, start_pos))
                current_pos = start_pos
    
    return segments

def find_intersections(segments: List[LineSegment]) -> List[Point]:
    """Find all intersection points and endpoints as vertices."""
    vertices = set()
    for seg1 in segments:
        p1, q1 = seg1
        vertices.add(p1)
        vertices.add(q1)
    
    for i, seg1 in enumerate(segments):
        for seg2 in segments[i+1:]:
            inter = line_intersection(seg1, seg2)
            if inter:
                vertices.add(inter)
    
    return list(vertices)

def line_intersection(seg1: LineSegment, seg2: LineSegment) -> Point or None:
    """Compute intersection of two line segments if exists."""
    a1, b1 = seg1
    a2, b2 = seg2
    denom = (b1[0] - a1[0]) * (b2[1] - a2[1]) - (b1[1] - a1[1]) * (b2[0] - a2[0])
    if abs(denom) < 1e-10:
        return None
    t = ((a2[0] - a1[0]) * (b2[1] - a2[1]) - (a2[1] - a1[1]) * (b2[0] - a2[0])) / denom
    u = -((b1[0] - a1[0]) * (a2[1] - a1[1]) - (b1[1] - a1[1]) * (a2[0] - a1[0])) / denom
    if 0 <= t <= 1 and 0 <= u <= 1:
        ix = a1[0] + t * (b1[0] - a1[0])
        iy = a1[1] + t * (b1[1] - a1[1])
        return (ix, iy)
    return None

def build_graph(vertices: List[Point], segments: List[LineSegment]) -> defaultdict:
    """Build adjacency list graph from vertices and segments."""
    # Map points to indices for graph
    point_to_idx = {p: i for i, p in enumerate(vertices)}
    graph = defaultdict(list)
    
    for seg in segments:
        i1 = point_to_idx.get(seg[0], None)
        i2 = point_to_idx.get(seg[1], None)
        if i1 is not None and i2 is not None and i1 != i2:
            graph[i1].append(i2)
            graph[i2].append(i1)
    
    return graph

def find_cycles(graph: defaultdict, start: int) -> List[List[int]]:
    """Find all elementary cycles using DFS (simple version, may find duplicates)."""
    cycles = []
    stack = [(start, [start], set([start]))]
    
    while stack:
        node, path, visited = stack.pop()
        for neighbor in graph[node]:
            if neighbor == start and len(path) > 2:
                cycles.append(path + [start])
            elif neighbor not in visited:
                new_visited = visited.copy()
                new_visited.add(neighbor)
                stack.append((neighbor, path + [neighbor], new_visited))
    
    return cycles  # Need to dedup and find all from all starts

def all_cycles(graph: defaultdict) -> List[List[int]]:
    """Find all unique cycles."""
    all_cycles_list = []
    visited_starts = set()
    for start in graph:
        if start not in visited_starts:
            cycles = find_cycles(graph, start)
            all_cycles_list.extend(cycles)
            visited_starts.update(c for cycle in cycles for c in cycle)
    # Dedup cycles by sorting and set
    unique_cycles = set(tuple(sorted(cycle)) for cycle in all_cycles_list if len(cycle) > 2)
    return [list(c) for c in unique_cycles]

def is_minimal_cycle(cycle: List[int], vertices: List[Point], graph: defaultdict) -> bool:
    """Check if cycle is minimal (no other vertices inside polygon)."""
    if len(cycle) < 3:
        return False
    poly_points = [vertices[i] for i in cycle]
    # Simple point-in-polygon for other vertices
    for v_idx in graph:
        if v_idx not in set(cycle):
            px, py = vertices[v_idx]
            if point_in_polygon((px, py), poly_points):
                return False
    return True

def point_in_polygon(point: Point, poly: List[Point]) -> bool:
    """Ray casting algorithm for point in polygon."""
    x, y = point
    n = len(poly)
    inside = False
    p1x, p1y = poly[0]
    for i in range(n + 1):
        p2x, p2y = poly[i % n]
        if y > min(p1y, p2y):
            if y <= max(p1y, p2y):
                if x <= max(p1x, p2x):
                    if p1y != p2y:
                        xinters = (y - p1y) * (p2x - p1x) / (p2y - p1y) + p1x
                        if p1x == p2x or x <= xinters:
                            inside = not inside
                    else:
                        # Skip horizontal edges
                        pass
        p1x, p1y = p2x, p2y
    return inside

def compute_bounding_box(points: List[Point]) -> List[float]:
    """Compute [x_min, y_min, x_max, y_max] for points."""
    if not points:
        return [0, 0, 0, 0]
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    return [min(xs), min(ys), max(xs), max(ys)]

def main():
    # Read SVG
    with open('test_blueprint_004_polygon.svg', 'r') as f:
        svg_content = f.read()
    
    # Parse segments from paths
    segments = parse_svg_paths(svg_content)
    print(f"Parsed {len(segments)} segments from paths")
    
    # Find vertices
    vertices = find_intersections(segments)
    print(f"Found {len(vertices)} vertices")
    
    # Build graph
    graph = build_graph(vertices, segments)
    
    # Find cycles
    cycles = all_cycles(graph)
    print(f"Found {len(cycles)} cycles")
    
    # Filter minimal
    minimal_cycles = [c for c in cycles if is_minimal_cycle(c, vertices, graph)]
    print(f"Found {len(minimal_cycles)} minimal cycles (rooms)")
    
    # Generate JSON
    rooms = []
    for i, cycle in enumerate(minimal_cycles):
        points = [vertices[j] for j in cycle]
        bbox = compute_bounding_box(points)
        rooms.append({
            "id": f"room_{i+1:03d}",
            "bounding_box": bbox,
            "name_hint": "Unknown"  # Could add ML for naming
        })
    
    output = {"rooms": rooms}
    with open('detected_rooms.json', 'w') as f:
        json.dump(output, f, indent=2)
    
    print("Output saved to detected_rooms.json")

if __name__ == "__main__":
    main()