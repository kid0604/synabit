<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue';
import * as d3 from 'd3';

const props = defineProps<{
  currentNoteId: string;
  currentNoteTitle: string;
  tags: string[];
  outgoingLinks: string[]; // IDs
  backlinks: Array<{ id: string, title: string }>;
  allNotes: Array<{ id: string, title: string }>;
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
    
    // Central node
    const centerNode = { id: props.currentNoteId, title: props.currentNoteTitle, group: 'center', radius: 10 };
    nodes.push(centerNode);

    // Track added nodes to avoid duplicates if a note is both outgoing and incoming
    const addedNodes = new Set<string>();
    addedNodes.add(props.currentNoteId);

    // Helper to add nodes
    const addNoteNode = (id: string, title: string, group: string) => {
        if (!addedNodes.has(id)) {
            nodes.push({ id, title, group, radius: 8 });
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
        addNoteNode(bl.id, bl.title, 'incoming');
        links.push({ source: bl.id, target: props.currentNoteId, type: 'incoming' });
    });

    // Tags
    props.tags.forEach(t => {
        const tagId = `tag-${t}`;
        if (!addedNodes.has(tagId)) {
            nodes.push({ id: tagId, title: `#${t}`, group: 'tag', radius: 6 });
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
                        nodes.push({ id: tagId, title: `#${t}`, group: 'tag', radius: 6 });
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
    const node = rootGroup.append("g")
        .selectAll("circle")
        .data(nodes)
        .join("circle")
        .attr("r", (d: any) => d.radius)
        .attr("fill", (d: any) => {
            if (d.group === 'center') return '#a855f7'; // Purple 500
            if (d.group === 'tag') return '#3b82f6'; // Blue 500
            if (d.group === 'related') return '#f59e0b'; // Amber 500 (related notes)
            return '#10b981'; // Emerald 500 (level 1 notes)
        })
        .attr("stroke", "#fff")
        .attr("stroke-width", 1.5)
        .style("cursor", "pointer")
        .call(d3.drag()
            .on("start", dragstarted)
            .on("drag", dragged)
            .on("end", dragended) as any);

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

    node.on("click", (event, d: any) => {
        if (d.group !== 'tag') {
            emit('open-note', d.id);
        }
    });

    simulation.on("tick", () => {
        link
            .attr("x1", (d: any) => d.source.x)
            .attr("y1", (d: any) => d.source.y)
            .attr("x2", (d: any) => d.target.x)
            .attr("y2", (d: any) => d.target.y);

        node
            .attr("cx", (d: any) => d.x)
            .attr("cy", (d: any) => d.y);
            
        labels
            .attr("x", (d: any) => d.x)
            .attr("y", (d: any) => d.y);
    });

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
        props.tags.slice().sort(),
        props.outgoingLinks.slice().sort(),
        props.backlinks.map(b => b.id).sort(),
        isShowMore.value
    ]);
};

let graphDebounceTimer: ReturnType<typeof setTimeout> | null = null;

watch(() => [props.currentNoteId, props.tags, props.outgoingLinks, props.backlinks, isShowMore.value], () => {
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
