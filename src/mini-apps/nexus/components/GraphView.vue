<script setup lang="ts">
import { ref, onMounted, watch, onUnmounted } from 'vue';
import * as d3 from 'd3';
import { Settings2, Eye, GitMerge, ListFilter, Focus } from 'lucide-vue-next';

interface GraphNode {
    id: string;
    item_type: string;
    title: string;
    tags: string[];
}

interface GraphLink {
    source: string;
    target: string;
}

interface GraphData {
    nodes: GraphNode[];
    links: GraphLink[];
}

const props = defineProps<{
    graphData: GraphData;
    /**
     * Ids matching the current filter query, or null when no query is running.
     * The search itself belongs to the parent — this component draws what it
     * is given — but an empty array is a real answer (nothing matched) and is
     * drawn as an empty graph, not as no filter.
     */
    matchIds?: string[] | null;
}>();

const emit = defineEmits<{
    (e: 'node-click', node: GraphNode): void
}>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const containerRef = ref<HTMLDivElement | null>(null);

// Settings State
const isPanelOpen = ref(false);
const activeTab = ref<'filters' | 'display' | 'forces'>('filters');

// Filters
const showNotes = ref(true);
const showTasks = ref(true);
const showEvents = ref(true);
const showTags = ref(true);
const showFiles = ref(false);
const showPeople = ref(true);
const showOrphans = ref(true);

// Display
const showLabels = ref(false);
const nodeSize = ref(1.0);
const linkThickness = ref(1.0);
/** Zoom level below which labels fade out. See the label pass in `draw`. */
const textFade = ref(0.4);

// Forces
const repelForce = ref(150);
const linkDist = ref(60);

interface SimNode extends d3.SimulationNodeDatum, GraphNode {
    val: number;
    degree: number;
}

interface SimLink extends d3.SimulationLinkDatum<SimNode> {
    source: SimNode;
    target: SimNode;
}

let simulation: d3.Simulation<SimNode, SimLink> | null = null;
let zoomBehavior: d3.ZoomBehavior<HTMLCanvasElement, unknown> | null = null;
let context: CanvasRenderingContext2D | null = null;
let resizeObserver: ResizeObserver | null = null;
let resizeFitTimeout: ReturnType<typeof setTimeout> | undefined;
let transform = d3.zoomIdentity;
let initialZoomSet = false;
let userInteracted = false;
let hoveredNode: SimNode | null = null;
let dragSubject: SimNode | null = null;
let currentNodes: SimNode[] = [];
let currentLinks: SimLink[] = [];

/**
 * How many nodes the last rebuild drew. Not the size of the match set: some
 * matches are of kinds the graph never shows, and tag and ghost nodes are
 * drawn without ever having matched anything.
 */
const drawnCount = ref(0);

/** Canvas size in CSS pixels. The backing store is this times the pixel ratio. */
let viewWidth = 0;
let viewHeight = 0;

/**
 * Where each node was the last time we looked. A rebuild — a filter toggled, or
 * the vault reloaded — restarts the layout from here rather than from random
 * positions, so the graph settles back into the picture the reader already knows
 * instead of scattering and re-forming.
 */
const positions = new Map<string, { x: number, y: number }>();

/**
 * Fingerprint of the last graph we laid out. The vault emits a reload on every
 * file change, and most of those leave the graph identical; comparing
 * fingerprints keeps those reloads from disturbing the layout at all.
 */
let lastSignature = '';

const colorMap: Record<string, string> = {
    'note': '#3b82f6',     // blue
    'task': '#10b981',     // emerald
    'event': '#f43f5e',    // rose
    'tag': '#a855f7',      // purple
    'file': '#8b5cf6',     // violet
    'person': '#f97316',   // orange
    'ghost': '#9ca3af',    // gray
};

const hashString = (h: number, s: string) => {
    for (let i = 0; i < s.length; i++) h = (Math.imul(h, 31) + s.charCodeAt(i)) | 0;
    return h;
};

const graphSignature = (data: GraphData) => {
    let h = 17;
    for (const n of data.nodes) {
        h = hashString(h, n.id);
        h = hashString(h, n.title);
        h = hashString(h, n.item_type);
    }
    for (const l of data.links) {
        h = hashString(h, l.source);
        h = hashString(h, l.target);
    }
    return `${data.nodes.length}:${data.links.length}:${h}`;
};

const rememberPositions = () => {
    for (const node of currentNodes) {
        if (node.x != null && node.y != null) {
            positions.set(node.id, { x: node.x, y: node.y });
        }
    }
};

/**
 * Tag and ghost nodes the query cannot speak for.
 *
 * A filter query is answered by the search index, which knows about notes,
 * tasks, events, files and people — the things that have text. Tags and ghosts
 * are derived: the graph invents them from properties and unresolved links, so
 * no query will ever return one. Dropping them would leave every match sitting
 * alone with the links that explain it cut away, so a derived node is kept
 * whenever something that did match is attached to it.
 */
const derivedNodesToKeep = (matched: Set<string>) => {
    const derivedTypes = new Map<string, string>();
    for (const node of props.graphData.nodes) {
        if (node.item_type === 'tag' || node.item_type === 'ghost') {
            derivedTypes.set(node.id, node.item_type);
        }
    }

    const keep = new Set<string>();
    for (const link of props.graphData.links) {
        if (matched.has(link.source) && derivedTypes.has(link.target)) keep.add(link.target);
        if (matched.has(link.target) && derivedTypes.has(link.source)) keep.add(link.source);
    }
    return keep;
};

const getFilteredData = () => {
    let nodes: SimNode[] = [];
    const links: SimLink[] = [];
    const nodeMap = new Map<string, SimNode>();
    let restoredCount = 0;

    const matched = props.matchIds ? new Set(props.matchIds) : null;
    const derivedKeep = matched ? derivedNodesToKeep(matched) : null;

    // Pass 1: Add allowed nodes
    for (const node of props.graphData.nodes) {
        // Exclude PDF annotations from the graph
        if (node.item_type.startsWith('pdf_')) continue;

        if (matched && !matched.has(node.id) && !derivedKeep!.has(node.id)) continue;

        if (!showNotes.value && (node.item_type === 'note' || node.item_type === 'ghost')) continue;
        if (!showTasks.value && node.item_type === 'task') continue;
        if (!showEvents.value && node.item_type === 'event') continue;
        if (!showFiles.value && node.item_type === 'file') continue;
        if (!showTags.value && node.item_type === 'tag') continue;
        if (!showPeople.value && node.item_type === 'person') continue;

        const known = positions.get(node.id);
        if (known) restoredCount += 1;

        const simNode: SimNode = {
            ...node,
            val: node.item_type === 'tag' ? 3 : 5,
            degree: 0,
            // Known nodes resume where they were; new ones scatter around 0,0.
            x: known ? known.x : (Math.random() - 0.5) * 500,
            y: known ? known.y : (Math.random() - 0.5) * 500
        };
        nodes.push(simNode);
        nodeMap.set(node.id, simNode);
    }

    // Pass 2: Add valid links
    for (const link of props.graphData.links) {
        const sourceNode = nodeMap.get(link.source);
        const targetNode = nodeMap.get(link.target);
        
        if (sourceNode && targetNode) {
            sourceNode.degree += 1;
            targetNode.degree += 1;
            
            sourceNode.val = Math.min(15, sourceNode.val + 0.2);
            targetNode.val = Math.min(15, targetNode.val + (targetNode.item_type === 'tag' ? 0.5 : 0.5));

            links.push({
                source: sourceNode,
                target: targetNode
            });
        }
    }

    if (!showOrphans.value) {
        nodes = nodes.filter(n => n.degree > 0);
    }

    // Most nodes already had a place: nudge the layout rather than re-running it.
    const settled = nodes.length > 0 && restoredCount / nodes.length > 0.5;

    return { nodes, links, settled };
};

/**
 * Size the backing store to the device's pixel ratio, not to CSS pixels — on a
 * Retina display the two differ by 2x, and drawing at CSS size makes every node
 * and label soft. `draw` compensates with a matching base transform.
 */
const sizeCanvas = (w: number, h: number) => {
    if (!canvasRef.value) return;
    const dpr = window.devicePixelRatio || 1;
    viewWidth = w;
    viewHeight = h;
    canvasRef.value.width = Math.round(w * dpr);
    canvasRef.value.height = Math.round(h * dpr);
};

/**
 * One-time wiring: canvas, zoom, drag, hover, resize, and an empty simulation.
 * Everything after this point mutates what is already here — nothing rebuilds
 * the machinery, because rebuilding it is what used to throw the layout away.
 */
const initCanvas = () => {
    if (!canvasRef.value || !containerRef.value) return false;

    const canvas = canvasRef.value;
    context = canvas.getContext('2d');
    if (!context) return false;

    const width = containerRef.value.clientWidth || window.innerWidth;
    const height = containerRef.value.clientHeight || window.innerHeight;
    sizeCanvas(width, height);

    // Zoom setup
    zoomBehavior = d3.zoom<HTMLCanvasElement, unknown>()
        .scaleExtent([0.1, 4])
        .extent([[0, 0], [width, height]])
        .on("zoom", (event) => {
            transform = event.transform;
            if (event.sourceEvent) userInteracted = true;
            draw();
        });

    if (!initialZoomSet && width > 0 && height > 0) {
        const initialScale = width < 768 ? 0.35 : 0.7;
        transform = d3.zoomIdentity
            .translate(width / 2, height / 2)
            .scale(initialScale);
        initialZoomSet = true;
    }

    d3.select(canvas)
        .call(zoomBehavior as any)
        .call(zoomBehavior.transform as any, transform)
        .on("dblclick.zoom", null);

    // Simulation Setup — created empty and stopped; `rebuildGraph` feeds it.
    simulation = d3.forceSimulation<SimNode>([])
        .force("link", d3.forceLink<SimNode, SimLink>([]).id(d => d.id).distance(linkDist.value))
        .force("charge", d3.forceManyBody<SimNode>().strength(-repelForce.value).distanceMax(400))
        .force("center", d3.forceCenter(0, 0))
        .force("x", d3.forceX<SimNode>(0).strength(0.05))
        .force("y", d3.forceY<SimNode>(0).strength(0.05))
        .force("collide", d3.forceCollide<SimNode>().radius(d => (d.val * 1.5 * nodeSize.value) + 5))
        .on("tick", () => draw());
    simulation.stop();

    // Resize observer
    const resizeState = { w: width, h: height };
    resizeObserver = new ResizeObserver(entries => {
        if (!entries || !entries.length) return;
        const { width: w, height: h } = entries[0].contentRect;
        if (w <= 0 || h <= 0) return;

        const dx = (w - resizeState.w) / 2;
        const dy = (h - resizeState.h) / 2;
        sizeCanvas(w, h);

        if (zoomBehavior && canvasRef.value) {
            zoomBehavior.extent([[0, 0], [w, h]]);
            transform = d3.zoomIdentity.translate(
                transform.x + dx,
                transform.y + dy
            ).scale(transform.k);
            d3.select(canvasRef.value).call(zoomBehavior.transform as any, transform);
        }

        simulation?.alpha(0.3).restart();

        if (!userInteracted) {
            clearTimeout(resizeFitTimeout);
            resizeFitTimeout = setTimeout(() => fitView(), 150);
        }

        resizeState.w = w;
        resizeState.h = h;
    });
    resizeObserver.observe(containerRef.value);

    // Canvas Interactions (Hover & Click)
    d3.select(canvas)
        .on("mousemove", (e) => {
            if (!simulation) return;
            const [x, y] = d3.pointer(e, canvas);
            const invX = transform.invertX(x);
            const invY = transform.invertY(y);
            const radiusSearch = 20 / transform.k;
            const found = simulation.find(invX, invY, radiusSearch);
            if (found !== hoveredNode) {
                hoveredNode = found || null;
                draw();
            }
        })
        .on("click", (_e) => {
            if (hoveredNode) {
                emit('node-click', hoveredNode);
            }
        });

    // Canvas Dragging
    const drag = d3.drag<HTMLCanvasElement, unknown>()
        .subject((e) => {
            if (!simulation) return null;
            const [x, y] = d3.pointer(e, canvas);
            const invX = transform.invertX(x);
            const invY = transform.invertY(y);
            dragSubject = simulation.find(invX, invY, 20 / transform.k) || null;
            return dragSubject;
        })
        .on("start", (e) => {
            if (!e.active) simulation?.alphaTarget(0.3).restart();
            if (dragSubject) {
                dragSubject.fx = dragSubject.x;
                dragSubject.fy = dragSubject.y;
            }
        })
        .on("drag", (e) => {
            if (dragSubject) {
                dragSubject.fx = transform.invertX(e.x);
                dragSubject.fy = transform.invertY(e.y);
            }
        })
        .on("end", (e) => {
            if (!e.active) simulation?.alphaTarget(0);
            if (dragSubject) {
                dragSubject.fx = null;
                dragSubject.fy = null;
                dragSubject = null;
            }
        });

    d3.select(canvas).call(drag as any);

    return true;
};

/** Filters or vault contents changed: new nodes and links, same simulation. */
const rebuildGraph = () => {
    if (!simulation) return;

    rememberPositions();
    const { nodes, links, settled } = getFilteredData();
    currentNodes = nodes;
    currentLinks = links;
    drawnCount.value = nodes.length;
    hoveredNode = null;

    const linkForce = simulation.force("link") as d3.ForceLink<SimNode, SimLink>;
    // Drop the old links first: they still point at the previous node objects,
    // and re-initialising the force against the new node array with stale
    // references produces garbage distances.
    linkForce.links([]);
    simulation.nodes(nodes);
    linkForce.links(links);

    simulation.alpha(settled ? 0.3 : 1).restart();
    draw();
};

/** Force sliders: re-heat gently, keeping the layout the reader is looking at. */
const updateForces = () => {
    if (!simulation) return;
    (simulation.force("link") as d3.ForceLink<SimNode, SimLink>).distance(linkDist.value);
    (simulation.force("charge") as d3.ForceManyBody<SimNode>).strength(-repelForce.value);
    simulation.alpha(0.3).restart();
};

/**
 * Display sliders are cosmetic: redraw, and re-seed the collision radius so a
 * later re-heat respects the new node size. No restart — moving nodes because
 * someone dragged a size slider is exactly the behaviour this replaces.
 */
const updateDisplay = () => {
    simulation?.force("collide", d3.forceCollide<SimNode>().radius(d => (d.val * 1.5 * nodeSize.value) + 5));
    draw();
};

const fitView = () => {
    if (!canvasRef.value || !zoomBehavior) return;
    const width = viewWidth || window.innerWidth;
    const height = viewHeight || window.innerHeight;
    
    // Determine bounds of current nodes
    if (currentNodes.length > 0) {
        const xExtent = d3.extent(currentNodes, d => d.x || 0) as [number, number];
        const yExtent = d3.extent(currentNodes, d => d.y || 0) as [number, number];
        const dx = xExtent[1] - xExtent[0];
        const dy = yExtent[1] - yExtent[0];
        const cx = (xExtent[0] + xExtent[1]) / 2;
        const cy = (yExtent[0] + yExtent[1]) / 2;
        
        const scale = Math.max(0.1, Math.min(2, 0.8 / Math.max(dx / width, dy / height)));
        
        d3.select(canvasRef.value).transition().duration(750).call(
            zoomBehavior.transform as any,
            d3.zoomIdentity
                .translate(width / 2, height / 2)
                .scale(scale)
                .translate(-cx, -cy)
        );
    } else {
        const initialScale = width < 768 ? 0.35 : 0.7;
        d3.select(canvasRef.value).transition().duration(750).call(
            zoomBehavior.transform as any,
            d3.zoomIdentity
                .translate(width / 2, height / 2)
                .scale(initialScale)
                .translate(-width / 2, -height / 2)
        );
    }
};

const draw = () => {
    const ctx = context;
    const canvas = canvasRef.value;
    if (!ctx || !canvas) return;

    // The ratio can change under us — dragging the window to a second monitor —
    // and nothing else would notice, so re-size on the way in.
    const dpr = window.devicePixelRatio || 1;
    if (viewWidth > 0 && canvas.width !== Math.round(viewWidth * dpr)) {
        sizeCanvas(viewWidth, viewHeight);
    }

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, viewWidth, viewHeight);
    ctx.translate(transform.x, transform.y);
    ctx.scale(transform.k, transform.k);

    const isHovering = !!hoveredNode;
    const connectedNodes = new Set<string>();
    
    if (isHovering) {
        connectedNodes.add(hoveredNode!.id);
        currentLinks.forEach(l => {
            if (l.source.id === hoveredNode!.id) connectedNodes.add(l.target.id);
            if (l.target.id === hoveredNode!.id) connectedNodes.add(l.source.id);
        });
    }

    // Draw Links
    ctx.lineWidth = 1.5 * linkThickness.value;
    currentLinks.forEach(link => {
        let alpha = 0.4;
        if (isHovering) {
            const connected = link.source.id === hoveredNode!.id || link.target.id === hoveredNode!.id;
            alpha = connected ? 1.0 : 0.05;
            ctx.strokeStyle = connected ? (colorMap[hoveredNode!.item_type] || '#999') : `rgba(153, 153, 153, ${alpha})`;
        } else {
            ctx.strokeStyle = `rgba(153, 153, 153, ${alpha})`;
        }
        
        ctx.beginPath();
        ctx.moveTo(link.source.x!, link.source.y!);
        ctx.lineTo(link.target.x!, link.target.y!);
        ctx.stroke();
    });

    // Draw Nodes
    currentNodes.forEach(node => {
        let alpha = 1.0;
        if (isHovering && !connectedNodes.has(node.id)) {
            alpha = 0.2;
        }
        
        ctx.beginPath();
        const r = node.val * 1.5 * nodeSize.value;
        ctx.moveTo(node.x! + r, node.y!);
        ctx.arc(node.x!, node.y!, r, 0, 2 * Math.PI);
        
        ctx.fillStyle = colorMap[node.item_type] || '#999';
        if (alpha < 1) ctx.globalAlpha = alpha;
        ctx.fill();
        
        if (node === hoveredNode) {
            ctx.lineWidth = 2;
            ctx.strokeStyle = '#000';
            ctx.stroke();
        } else {
            ctx.lineWidth = 1.5;
            ctx.strokeStyle = '#fff';
            ctx.stroke();
        }
        ctx.globalAlpha = 1.0;
    });

    // Draw Labels
    //
    // Text is sized in screen space: the canvas is scaled by `transform.k`, so
    // dividing it back out keeps a label the same size however far in or out the
    // reader has zoomed. Zoomed far out there is no room for text at all, so
    // labels fade to nothing below the threshold — only the hovered node keeps
    // its own, which is the only one being asked for down there.
    const k = transform.k;
    const fade = Math.min(1, Math.max(0, (k - textFade.value) / 0.3));
    currentNodes.forEach(node => {
        const isHovered = node === hoveredNode;
        if (!isHovered) {
            if (fade <= 0) return;
            if (!showLabels.value && node.val <= 6) return;
        }

        let alpha = isHovered ? 1.0 : fade;
        if (isHovering && !connectedNodes.has(node.id)) {
            alpha *= 0.2;
        }

        const fontPx = Math.max(10, node.val * 1.2 * nodeSize.value) / k;
        ctx.font = `${isHovered ? 'bold ' : ''}${fontPx}px sans-serif`;
        ctx.fillStyle = `rgba(100, 100, 100, ${alpha})`;
        const r = node.val * 1.5 * nodeSize.value;
        ctx.fillText(node.title, node.x! + r + (4 / k), node.y! + (fontPx * 0.35));
    });
};

// Filters change which nodes exist: rebuild, restarting from known positions.
watch([showNotes, showTasks, showEvents, showTags, showFiles, showPeople, showOrphans], () => {
    rebuildGraph();
});

// The query result arrives from the parent as a new array each time.
watch(() => props.matchIds, () => rebuildGraph());

// A vault reload only matters if the graph actually changed.
watch(() => props.graphData, (data) => {
    const signature = graphSignature(data);
    if (signature === lastSignature) return;
    lastSignature = signature;
    rebuildGraph();
});

// Forces re-heat the existing layout; display settings only redraw.
watch([repelForce, linkDist], () => updateForces());
watch([showLabels, nodeSize, linkThickness, textFade], () => updateDisplay());

onMounted(() => {
    setTimeout(() => {
        if (!initCanvas()) return;
        lastSignature = graphSignature(props.graphData);
        rebuildGraph();
    }, 100);
});

onUnmounted(() => {
    clearTimeout(resizeFitTimeout);
    simulation?.stop();
    simulation = null;
    resizeObserver?.disconnect();
    resizeObserver = null;
});

</script>

<template>
    <div ref="containerRef" class="w-full h-full relative overflow-hidden bg-[#fdfdfc] dark:bg-[#1a1a1c] select-none" @click="isPanelOpen = false">
        <canvas ref="canvasRef" class="w-full h-full cursor-grab active:cursor-grabbing"></canvas>
        
        <!-- Toggle Button -->
        <button 
            @click.stop="isPanelOpen = !isPanelOpen" 
            class="absolute top-[100px] right-6 z-20 w-10 h-10 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md rounded-full shadow-lg flex items-center justify-center border border-gray-200 dark:border-[#3a3a3c] hover:bg-gray-50 dark:hover:bg-[#3a3a3c] transition-all"
            :class="{ 'rotate-90': isPanelOpen }"
         aria-label="Is Panel Open = !is Panel Open">
            <Settings2 class="w-5 h-5 text-gray-700 dark:text-gray-300" />
        </button>

        <!-- Match count, while the search is narrowing the graph -->
        <div
            v-if="matchIds"
            class="absolute bottom-24 left-6 z-20 px-3 py-1.5 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md rounded-full shadow-lg border border-gray-200 dark:border-[#3a3a3c] text-xs font-semibold text-gray-600 dark:text-gray-300"
        >
            {{ drawnCount }} {{ drawnCount === 1 ? 'node' : 'nodes' }}
        </div>

        <!-- Fit View Button -->
        <button 
            @click.stop="fitView" 
            class="absolute bottom-24 right-6 z-20 w-10 h-10 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md rounded-full shadow-lg flex items-center justify-center border border-gray-200 dark:border-[#3a3a3c] hover:bg-gray-50 dark:hover:bg-[#3a3a3c] transition-all"
            title="Fit to Screen"
        >
            <Focus class="w-5 h-5 text-gray-700 dark:text-gray-300" />
        </button>
        
        <!-- Obsidian-Style Floating Settings Panel -->
        <div 
            v-show="isPanelOpen"
            @click.stop
            class="absolute top-[150px] right-6 z-20 w-80 max-h-[calc(100vh-180px)] flex flex-col bg-white/95 dark:bg-[#1e1e20]/95 backdrop-blur-2xl rounded-2xl shadow-2xl border border-gray-200/50 dark:border-[#3a3a3c]/50 overflow-hidden animate-in slide-in-from-right-8 duration-300"
        >
            <!-- Tabs Header -->
            <div class="flex border-b border-gray-200 dark:border-[#3a3a3c]">
                <button 
                    @click="activeTab = 'filters'" 
                    class="flex-1 py-3 px-2 flex items-center justify-center gap-2 text-xs font-bold tracking-wider uppercase transition-colors"
                    :class="activeTab === 'filters' ? 'text-indigo-600 dark:text-indigo-400 border-b-2 border-indigo-600 dark:border-indigo-400 bg-indigo-50/50 dark:bg-indigo-900/20' : 'text-gray-500 hover:bg-gray-50 dark:hover:bg-white/5'"
                >
                    <ListFilter class="w-4 h-4" /> Filters
                </button>
                <button 
                    @click="activeTab = 'display'" 
                    class="flex-1 py-3 px-2 flex items-center justify-center gap-2 text-xs font-bold tracking-wider uppercase transition-colors"
                    :class="activeTab === 'display' ? 'text-indigo-600 dark:text-indigo-400 border-b-2 border-indigo-600 dark:border-indigo-400 bg-indigo-50/50 dark:bg-indigo-900/20' : 'text-gray-500 hover:bg-gray-50 dark:hover:bg-white/5'"
                >
                    <Eye class="w-4 h-4" /> Display
                </button>
                <button 
                    @click="activeTab = 'forces'" 
                    class="flex-1 py-3 px-2 flex items-center justify-center gap-2 text-xs font-bold tracking-wider uppercase transition-colors"
                    :class="activeTab === 'forces' ? 'text-indigo-600 dark:text-indigo-400 border-b-2 border-indigo-600 dark:border-indigo-400 bg-indigo-50/50 dark:bg-indigo-900/20' : 'text-gray-500 hover:bg-gray-50 dark:hover:bg-white/5'"
                >
                    <GitMerge class="w-4 h-4" /> Forces
                </button>
            </div>

            <!-- Tab Contents -->
            <div class="p-6 overflow-y-auto">
                <!-- Filters Tab -->
                <div v-show="activeTab === 'filters'" class="space-y-4">
                    <h3 class="text-xs font-semibold text-gray-400 mb-3 uppercase tracking-wider">Node Types</h3>
                    <label class="flex items-center justify-between cursor-pointer group">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full" :style="{ backgroundColor: showNotes ? colorMap['note'] : '#e5e7eb' }"></div>
                            <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Notes</span>
                        </div>
                        <input type="checkbox" v-model="showNotes" class="toggle-checkbox" />
                    </label>
                    <label class="flex items-center justify-between cursor-pointer group">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full" :style="{ backgroundColor: showTasks ? colorMap['task'] : '#e5e7eb' }"></div>
                            <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Tasks</span>
                        </div>
                        <input type="checkbox" v-model="showTasks" class="toggle-checkbox" />
                    </label>
                    <label class="flex items-center justify-between cursor-pointer group">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full" :style="{ backgroundColor: showEvents ? colorMap['event'] : '#e5e7eb' }"></div>
                            <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Events</span>
                        </div>
                        <input type="checkbox" v-model="showEvents" class="toggle-checkbox" />
                    </label>
                    <label class="flex items-center justify-between cursor-pointer group">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full" :style="{ backgroundColor: showTags ? colorMap['tag'] : '#e5e7eb' }"></div>
                            <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Tags</span>
                        </div>
                        <input type="checkbox" v-model="showTags" class="toggle-checkbox" />
                    </label>
                    <label class="flex items-center justify-between cursor-pointer group">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full" :style="{ backgroundColor: showFiles ? colorMap['file'] : '#e5e7eb' }"></div>
                            <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Files</span>
                        </div>
                        <input type="checkbox" v-model="showFiles" class="toggle-checkbox" />
                    </label>
                    <label class="flex items-center justify-between cursor-pointer group">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full" :style="{ backgroundColor: showPeople ? colorMap['person'] : '#e5e7eb' }"></div>
                            <span class="text-sm font-medium text-gray-700 dark:text-gray-200">People</span>
                        </div>
                        <input type="checkbox" v-model="showPeople" class="toggle-checkbox" />
                    </label>

                    <div class="h-px bg-gray-200 dark:bg-[#3a3a3c] my-4"></div>
                    
                    <label class="flex items-center justify-between cursor-pointer group">
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Show Orphans</span>
                        <input type="checkbox" v-model="showOrphans" class="toggle-checkbox" />
                    </label>
                    <p class="text-[11px] text-gray-400 mt-1">Show nodes without any links</p>
                </div>

                <!-- Display Tab -->
                <div v-show="activeTab === 'display'" class="space-y-6">
                    <label class="flex items-center justify-between cursor-pointer group">
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-200">Show Labels</span>
                        <input type="checkbox" v-model="showLabels" class="toggle-checkbox" />
                    </label>

                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Node Size</label>
                            <span class="text-xs text-gray-500 font-mono">{{ nodeSize.toFixed(1) }}x</span>
                        </div>
                        <input type="range" v-model.number="nodeSize" min="0.5" max="3" step="0.1" class="w-full range-slider" aria-label="Node size" />
                    </div>

                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Link Thickness</label>
                            <span class="text-xs text-gray-500 font-mono">{{ linkThickness.toFixed(1) }}x</span>
                        </div>
                        <input type="range" v-model.number="linkThickness" min="0.5" max="3" step="0.1" class="w-full range-slider" aria-label="Link thickness" />
                    </div>

                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Text Fade Threshold</label>
                            <span class="text-xs text-gray-500 font-mono">{{ textFade.toFixed(1) }}x</span>
                        </div>
                        <input type="range" v-model.number="textFade" min="0.1" max="2" step="0.1" class="w-full range-slider" aria-label="Text fade threshold" />
                        <p class="text-[11px] text-gray-400">Hide labels when zoomed out past this level</p>
                    </div>
                </div>

                <!-- Forces Tab -->
                <div v-show="activeTab === 'forces'" class="space-y-6">
                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Repel Force</label>
                            <span class="text-xs text-gray-500 font-mono">{{ repelForce }}</span>
                        </div>
                        <input type="range" v-model.number="repelForce" min="50" max="400" step="10" class="w-full range-slider" aria-label="Repel force" />
                        <p class="text-[11px] text-gray-400">Push nodes further apart</p>
                    </div>

                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Link Distance</label>
                            <span class="text-xs text-gray-500 font-mono">{{ linkDist }}</span>
                        </div>
                        <input type="range" v-model.number="linkDist" min="20" max="150" step="5" class="w-full range-slider" aria-label="Link distance" />
                        <p class="text-[11px] text-gray-400">Length of links between nodes</p>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
/* Custom Toggle Switch */
.toggle-checkbox {
    appearance: none;
    width: 36px;
    height: 20px;
    background-color: #e5e7eb;
    border-radius: 9999px;
    position: relative;
    cursor: pointer;
    outline: none;
    transition: background-color 0.2s;
}
.dark .toggle-checkbox {
    background-color: #3f3f46;
}
.toggle-checkbox:checked {
    background-color: #4f46e5;
}
.toggle-checkbox::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background-color: white;
    border-radius: 50%;
    transition: transform 0.2s;
    box-shadow: 0 1px 3px rgba(0,0,0,0.1);
}
.toggle-checkbox:checked::after {
    transform: translateX(16px);
}

/* Custom Range Slider */
.range-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 4px;
    background: #e5e7eb;
    border-radius: 4px;
    outline: none;
}
.dark .range-slider {
    background: #3f3f46;
}
.range-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #4f46e5;
    cursor: pointer;
    border: 2px solid white;
    box-shadow: 0 1px 3px rgba(0,0,0,0.2);
    transition: transform 0.1s;
}
.dark .range-slider::-webkit-slider-thumb {
    border-color: #1e1e20;
}
.range-slider::-webkit-slider-thumb:hover {
    transform: scale(1.2);
}
</style>
