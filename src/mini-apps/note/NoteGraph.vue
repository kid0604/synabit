<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue';
import * as d3 from 'd3';

import { iconBodyForNodeType } from '../../shared/views/nodeTypeIcon';

const props = defineProps<{
  currentNoteId: string;
  currentNoteTitle: string;
  tags: string[];
  outgoingLinks: string[]; // IDs
  backlinks: Array<{ id: string, title: string, nodeType?: string }>;
  allNotes: Array<{ id: string, title: string, tags?: string[], nodeType?: string }>;
  /** The open node's own kind, for the mark in the middle. */
  currentNodeType?: string;
  /**
   * Discs, or the kind's icon on the disc.
   *
   * Discs by default: Notes draws one kind of thing, so a glyph would say the
   * same word on every mark. Things draws whatever the vault holds — a graph
   * around a `book` reaches people, tasks and notes — and there the shape is
   * the fastest way to read what a neighbour is.
   *
   * The disc stays underneath either way. It carries the colour the legend
   * explains, and dropping it would trade one fact for another.
   */
  marks?: 'dots' | 'icons';
}>();

const emit = defineEmits<{
  (e: 'open-note', id: string): void;
}>();

const svgRef = ref<SVGSVGElement | null>(null);
const isShowMore = ref(false);
let simulation: d3.Simulation<d3.SimulationNodeDatum, undefined> | null = null;
let resizeObserver: ResizeObserver | null = null;

const renderGraph = () => {
    if (!svgRef.value) return;
    const svg = d3.select(svgRef.value);
    svg.selectAll("*").remove();

    const width = svgRef.value.clientWidth || 300;
    const height = svgRef.value.clientHeight || 300;

    // Build unique nodes and links
    const nodes: any[] = [];
    const links: any[] = [];
    
    const cx = width / 2;
    const cy = height / 2;

    // Central node
    const centerNode = { id: props.currentNoteId, title: props.currentNoteTitle, group: 'center', radius: 10, x: cx, y: cy, nodeType: props.currentNodeType };
    nodes.push(centerNode);

    // Track added nodes to avoid duplicates if a note is both outgoing and incoming
    const addedNodes = new Set<string>();
    addedNodes.add(props.currentNoteId);

    // Helper to add nodes
    const addNoteNode = (id: string, title: string, group: string, nodeType?: string) => {
        if (!addedNodes.has(id)) {
            // Falls back to whatever the caller knows about the node elsewhere,
            // so a backlink and a neighbour draw the same mark for one node.
            const known = nodeType ?? props.allNotes.find(n => n.id === id)?.nodeType;
            nodes.push({ id, title, group, radius: 8, x: cx, y: cy, nodeType: known });
            addedNodes.add(id);
        }
    };

    // Outgoing
    props.outgoingLinks.forEach(id => {
        const found = props.allNotes.find(n => n.id === id);
        const title = found ? found.title : id.split(/[\\/]/).pop() || id;
        addNoteNode(id, title, 'outgoing');
        links.push({ source: props.currentNoteId, target: id, type: 'outgoing' });
    });

    // Incoming (backlinks)
    props.backlinks.forEach(bl => {
        addNoteNode(bl.id, bl.title, 'incoming', bl.nodeType);
        links.push({ source: bl.id, target: props.currentNoteId, type: 'incoming' });
    });

    // Tags
    props.tags.forEach(t => {
        const tagId = `tag-${t}`;
        if (!addedNodes.has(tagId)) {
            nodes.push({ id: tagId, title: `#${t}`, group: 'tag', radius: 6, x: cx, y: cy });
            addedNodes.add(tagId);
        }
        links.push({ source: props.currentNoteId, target: tagId, type: 'tag' });
    });

    if (isShowMore.value) {
        // Expand Tags: notes that share current note's tags
        props.tags.forEach(t => {
            const tagId = `tag-${t}`;
            props.allNotes.forEach(n => {
                if (n.id !== props.currentNoteId && n.tags && n.tags.includes(t)) {
                    addNoteNode(n.id, n.title, 'related');
                    links.push({ source: tagId, target: n.id, type: 'related-tag' });
                }
            });
        });

        // Expand Level 1 Notes: tags of the notes connected to current note
        const level1NoteIds = [...props.outgoingLinks, ...props.backlinks.map(b => b.id)];
        level1NoteIds.forEach(id => {
            const nodeMeta = props.allNotes.find(n => n.id === id);
            if (nodeMeta && nodeMeta.tags) {
                nodeMeta.tags.forEach(t => {
                    const tagId = `tag-${t}`;
                    if (!addedNodes.has(tagId)) {
                        nodes.push({ id: tagId, title: `#${t}`, group: 'tag', radius: 6, x: cx, y: cy });
                        addedNodes.add(tagId);
                    }
                    links.push({ source: id, target: tagId, type: 'related-tag' });
                });
            }
        });
    }

    // Setup simulation
    simulation = d3.forceSimulation(nodes)
        .force("link", d3.forceLink(links).id((d: any) => d.id).distance(isShowMore.value ? 80 : 60))
        .force("charge", d3.forceManyBody().strength(isShowMore.value ? -200 : -150))
        .force("center", d3.forceCenter(width / 2, height / 2).strength(0.1))
        .force("collide", d3.forceCollide().radius((d: any) => d.radius + 10).iterations(2));

    // Arrow marker
    svg.append("defs").append("marker")
        .attr("id", "arrow")
        .attr("viewBox", "0 -5 10 10")
        .attr("refX", 18) // position it so it touches the radius
        .attr("refY", 0)
        .attr("markerWidth", 6)
        .attr("markerHeight", 6)
        .attr("orient", "auto")
        .append("path")
        .attr("d", "M0,-5L10,0L0,5")
        .attr("fill", "#9ca3af");
        
    svg.select("defs").append("marker")
        .attr("id", "arrow-dark")
        .attr("viewBox", "0 -5 10 10")
        .attr("refX", 18)
        .attr("refY", 0)
        .attr("markerWidth", 6)
        .attr("markerHeight", 6)
        .attr("orient", "auto")
        .append("path")
        .attr("d", "M0,-5L10,0L0,5")
        .attr("fill", "#52525b");

    const rootGroup = svg.append("g");
    
    // Zoom behavior
    const zoom = d3.zoom().scaleExtent([0.2, 4]).on("zoom", (e) => {
        rootGroup.attr("transform", e.transform);
    });
    svg.call(zoom as any);

    // Draw links
    const link = rootGroup.append("g")
        .selectAll("line")
        .data(links)
        .join("line")
        .attr("stroke", "currentColor")
        .attr("stroke-opacity", (d: any) => d.type === 'related-tag' ? 0.2 : 0.4)
        .attr("stroke-dasharray", (d: any) => d.type === 'related-tag' ? "2,2" : "none")
        .attr("stroke-width", 1.5)
        .attr("class", "text-gray-400 dark:text-zinc-600")
        .attr("marker-end", (d: any) => {
            if (d.type === 'related-tag') return null; // No arrow for secondary relations
            return document.documentElement.classList.contains('dark') ? "url(#arrow-dark)" : "url(#arrow)";
        });

    // Draw nodes
    //
    // A `<g>` per node rather than a bare circle, so the mark can be a disc or
    // a disc with a glyph on it and the tick below still has one thing to
    // move. The circle sits at the group's origin, which is what the translate
    // then places — identical to the old `cx`/`cy` for anybody drawing discs.
    const colourFor = (d: any) => {
        if (d.group === 'center') return '#a855f7'; // Purple 500
        if (d.group === 'tag') return '#3b82f6'; // Blue 500
        if (d.group === 'related') return '#f59e0b'; // Amber 500 (related notes)
        return '#10b981'; // Emerald 500 (level 1 notes)
    };

    const showIcons = props.marks === 'icons';

    const node = rootGroup.append("g")
        .selectAll("g")
        .data(nodes)
        .join("g")
        .style("cursor", "pointer")
        .call(d3.drag()
            .on("start", dragstarted)
            .on("drag", dragged)
            .on("end", dragended) as any);

    node.append("circle")
        // Room for the glyph, which is drawn inside it.
        .attr("r", (d: any) => (showIcons && d.group !== 'tag' ? d.radius + 2 : d.radius))
        .attr("fill", colourFor)
        .attr("stroke", "#fff")
        .attr("stroke-width", 1.5);

    if (showIcons) {
        // Tags keep a plain disc. A tag is not a node and has no kind, so
        // there is no icon that would be true — and the difference reads as
        // the distinction it is rather than as something missing.
        node.filter((d: any) => d.group !== 'tag')
            .append("g")
            .attr("fill", "none")
            .attr("stroke", "#fff")
            .attr("stroke-width", 2.4)
            .attr("stroke-linecap", "round")
            .attr("stroke-linejoin", "round")
            .attr("pointer-events", "none")
            .attr("transform", (d: any) => {
                // Lucide draws on a 24-grid; fit it inside the disc and put
                // its middle on the group's origin.
                const size = (d.radius + 2) * 1.5;
                return `translate(${-size / 2},${-size / 2}) scale(${size / 24})`;
            })
            .html((d: any) => iconBodyForNodeType(d.nodeType ?? ''));
    }

    // Add titles
    node.append("title")
        .text((d: any) => d.title);
        
    // Add labels for tags and center node
    const labels = rootGroup.append("g")
        .selectAll("text")
        .data(nodes)
        .join("text")
        .text((d: any) => {
            if (d.group === 'center') return d.title.length > 15 ? d.title.substring(0,15)+"..." : d.title;
            if (d.group === 'tag') return d.title;
            return "";
        })
        .attr("font-size", "10px")
        .attr("fill", "currentColor")
        .attr("class", "text-gray-700 dark:text-gray-300 pointer-events-none")
        .attr("text-anchor", "middle")
        .attr("dy", 18);

    node.on("click", (_event, d: any) => {
        if (d.group !== 'tag') {
            emit('open-note', d.id);
        }
    });

    // Stop simulation from auto-running to prevent the fly-in animation
    simulation.stop();
    
    // Fast-forward simulation to a stable layout (300 ticks is usually enough)
    simulation.tick(300);

    const ticked = () => {
        link
            .attr("x1", (d: any) => d.source.x)
            .attr("y1", (d: any) => d.source.y)
            .attr("x2", (d: any) => d.target.x)
            .attr("y2", (d: any) => d.target.y);

        node
            .attr("transform", (d: any) => `translate(${d.x},${d.y})`);
            
        labels
            .attr("x", (d: any) => d.x)
            .attr("y", (d: any) => d.y);
    };

    // Render the initial static layout
    ticked();

    // Listen for future ticks (e.g. during dragging)
    simulation.on("tick", ticked);

    function dragstarted(event: any, d: any) {
        if (!event.active) simulation?.alphaTarget(0.3).restart();
        d.fx = d.x;
        d.fy = d.y;
    }

    function dragged(event: any, d: any) {
        d.fx = event.x;
        d.fy = event.y;
    }

    function dragended(event: any, d: any) {
        if (!event.active) simulation?.alphaTarget(0);
        d.fx = null;
        d.fy = null;
    }
};

let lastGraphFingerprint = '';

const computeGraphFingerprint = () => {
    return JSON.stringify([
        props.currentNoteId,
        props.currentNoteTitle,
        props.tags.slice().sort(),
        props.outgoingLinks.slice().sort(),
        props.backlinks.map(b => b.id).sort(),
        isShowMore.value,
        // How the marks are drawn is part of the picture. Left out, flipping
        // the switch would change nothing until something else moved — which
        // reads as the switch being broken.
        props.marks ?? 'dots',
    ]);
};

let graphDebounceTimer: ReturnType<typeof setTimeout> | null = null;

watch(() => [props.currentNoteId, props.currentNoteTitle, props.tags, props.outgoingLinks, props.backlinks, isShowMore.value, props.marks], () => {
    const fingerprint = computeGraphFingerprint();
    if (fingerprint === lastGraphFingerprint) return;
    lastGraphFingerprint = fingerprint;
    
    // Debounce to avoid rapid re-renders when switching notes
    if (graphDebounceTimer) clearTimeout(graphDebounceTimer);
    graphDebounceTimer = setTimeout(() => {
        renderGraph();
    }, 150);
}, { deep: true });

// Listen for dark mode toggle to update arrow markers
const onThemeChange = () => renderGraph();

onMounted(() => {
    renderGraph();
    
    // Add resize observer
    if (svgRef.value) {
        resizeObserver = new ResizeObserver(() => {
            renderGraph();
        });
        resizeObserver.observe(svgRef.value.parentElement!);
    }
    
    // mutation observer for class changes on html
    const observer = new MutationObserver((mutations) => {
        for (const m of mutations) {
            if (m.attributeName === 'class') {
                onThemeChange();
            }
        }
    });
    observer.observe(document.documentElement, { attributes: true });
});

onUnmounted(() => {
    if (simulation) simulation.stop();
    if (resizeObserver) resizeObserver.disconnect();
});
</script>

<template>
  <div class="w-full h-full relative cursor-grab active:cursor-grabbing">
    <svg ref="svgRef" class="w-full h-full"></svg>
    
    <div class="absolute top-2 right-2">
       <button 
           @click="isShowMore = !isShowMore" 
           class="px-2 py-1 bg-white dark:bg-[#2c2c2c] border border-gray-200 dark:border-[#3f3f46] text-xs font-medium text-gray-600 dark:text-gray-300 rounded shadow-sm hover:bg-gray-50 dark:hover:bg-[#3f3f46] transition-colors"
       >
           {{ isShowMore ? 'Show Less' : 'Show More' }}
       </button>
    </div>

    <div class="absolute bottom-2 left-2 flex gap-3 text-[10px] text-gray-500 font-medium">
       <div class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-purple-500 inline-block"></span> Current</div>
       <div class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-emerald-500 inline-block"></span> Linked</div>
       <div class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-blue-500 inline-block"></span> Tag</div>
       <div v-if="isShowMore" class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-amber-500 inline-block"></span> Related Note</div>
    </div>
  </div>
</template>
