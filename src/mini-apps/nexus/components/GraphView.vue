<script setup lang="ts">
import { ref, onMounted, watch, onUnmounted } from 'vue';
import * as d3 from 'd3';
import { Settings2, Eye, GitMerge, ListFilter } from 'lucide-vue-next';

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

let simulation: d3.Simulation<SimNode, SimLink>;
let zoomBehavior: d3.ZoomBehavior<HTMLCanvasElement, unknown>;
let transform = d3.zoomIdentity;
let initialZoomSet = false;
let hoveredNode: SimNode | null = null;
let dragSubject: SimNode | null = null;
let currentNodes: SimNode[] = [];
let currentLinks: SimLink[] = [];

const colorMap: Record<string, string> = {
    'note': '#3b82f6',     // blue
    'task': '#10b981',     // emerald
    'event': '#f43f5e',    // rose
    'tag': '#a855f7',      // purple
    'file': '#8b5cf6',     // violet
    'person': '#f97316',   // orange
    'ghost': '#9ca3af',    // gray
};

const getFilteredData = () => {
    let nodes: SimNode[] = [];
    const links: SimLink[] = [];
    const nodeMap = new Map<string, SimNode>();

    // Pass 1: Add allowed nodes
    for (const node of props.graphData.nodes) {
        // Exclude PDF annotations from the graph
        if (node.item_type.startsWith('pdf_')) continue;

        if (!showNotes.value && (node.item_type === 'note' || node.item_type === 'ghost')) continue;
        if (!showTasks.value && node.item_type === 'task') continue;
        if (!showEvents.value && node.item_type === 'event') continue;
        if (!showFiles.value && node.item_type === 'file') continue;
        if (!showTags.value && node.item_type === 'tag') continue;
        if (!showPeople.value && node.item_type === 'person') continue;

        const simNode: SimNode = {
            ...node,
            val: node.item_type === 'tag' ? 3 : 5,
            degree: 0,
            x: Math.random() * 500, // scatter start positions slightly
            y: Math.random() * 500
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

    return { nodes, links };
};

const renderGraph = () => {
    if (!canvasRef.value || !containerRef.value) return;
    
    const { nodes, links } = getFilteredData();
    currentNodes = nodes;
    currentLinks = links;
    
    const width = containerRef.value.clientWidth || window.innerWidth;
    const height = containerRef.value.clientHeight || window.innerHeight;
    
    const canvas = canvasRef.value;
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    if (!context) return;

    // Zoom setup
    zoomBehavior = d3.zoom<HTMLCanvasElement, unknown>()
        .scaleExtent([0.1, 4])
        .on("zoom", (event) => {
            transform = event.transform;
            draw(context);
        });

    if (!initialZoomSet && width > 0 && height > 0) {
        transform = d3.zoomIdentity
            .translate(width / 2, height / 2)
            .scale(0.7)
            .translate(-width / 2, -height / 2);
        initialZoomSet = true;
    }

    d3.select(canvas)
        .call(zoomBehavior as any)
        .call(zoomBehavior.transform as any, transform)
        .on("dblclick.zoom", null);

    // Simulation Setup
    if (simulation) simulation.stop();
    
    simulation = d3.forceSimulation<SimNode>(nodes)
        .force("link", d3.forceLink<SimNode, SimLink>(links).id(d => d.id).distance(linkDist.value))
        .force("charge", d3.forceManyBody().strength(-repelForce.value).distanceMax(400))
        .force("center", d3.forceCenter(width / 2, height / 2))
        .force("x", d3.forceX(width / 2).strength(0.05))
        .force("y", d3.forceY(height / 2).strength(0.05))
        .force("collide", d3.forceCollide().radius(d => ((d as SimNode).val * 1.5 * nodeSize.value) + 5));

    simulation.on("tick", () => {
        draw(context);
    });

    // Resize observer
    const resizeObserver = new ResizeObserver(entries => {
        if (!entries || !entries.length) return;
        const { width: w, height: h } = entries[0].contentRect;
        if (w > 0 && h > 0) {
            canvas.width = w;
            canvas.height = h;
            simulation.force("center", d3.forceCenter(w / 2, h / 2));
            simulation.force("x", d3.forceX(w / 2).strength(0.05));
            simulation.force("y", d3.forceY(h / 2).strength(0.05));
            simulation.alpha(0.3).restart();
        }
    });
    resizeObserver.observe(containerRef.value);

    // Canvas Interactions (Hover & Click)
    d3.select(canvas)
        .on("mousemove", (e) => {
            const [x, y] = d3.pointer(e, canvas);
            const invX = transform.invertX(x);
            const invY = transform.invertY(y);
            const radiusSearch = 20 / transform.k;
            const found = simulation.find(invX, invY, radiusSearch);
            if (found !== hoveredNode) {
                hoveredNode = found || null;
                draw(context);
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
            const [x, y] = d3.pointer(e, canvas);
            const invX = transform.invertX(x);
            const invY = transform.invertY(y);
            dragSubject = simulation.find(invX, invY, 20 / transform.k) || null;
            return dragSubject;
        })
        .on("start", (e) => {
            if (!e.active) simulation.alphaTarget(0.3).restart();
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
            if (!e.active) simulation.alphaTarget(0);
            if (dragSubject) {
                dragSubject.fx = null;
                dragSubject.fy = null;
                dragSubject = null;
            }
        });
        
    d3.select(canvas).call(drag as any);

    return () => resizeObserver.disconnect();
};

const draw = (ctx: CanvasRenderingContext2D) => {
    ctx.save();
    ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
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
    currentNodes.forEach(node => {
        const forceShow = node === hoveredNode || showLabels.value || node.val > 6;
        if (!forceShow) return;

        let alpha = 1.0;
        if (isHovering && !connectedNodes.has(node.id)) {
            alpha = 0.2;
        }

        ctx.font = `${node === hoveredNode ? 'bold ' : ''}${Math.max(10, node.val * 1.2 * nodeSize.value)}px sans-serif`;
        ctx.fillStyle = `rgba(100, 100, 100, ${alpha})`;
        const r = node.val * 1.5 * nodeSize.value;
        ctx.fillText(node.title, node.x! + r + 4, node.y! + 4);
    });

    ctx.restore();
};

let cleanupObserver: (() => void) | null = null;

watch([
    showNotes, showTasks, showEvents, showTags, showFiles, showPeople, showOrphans, 
    showLabels, nodeSize, linkThickness, repelForce, linkDist, 
    () => props.graphData
], () => {
    if (cleanupObserver) cleanupObserver();
    cleanupObserver = renderGraph() || null;
}, { deep: true });

onMounted(() => {
    setTimeout(() => {
        cleanupObserver = renderGraph() || null;
    }, 100);
});

onUnmounted(() => {
    if (simulation) {
        simulation.stop();
    }
    if (cleanupObserver) cleanupObserver();
});

</script>

<template>
    <div ref="containerRef" class="w-full h-full relative overflow-hidden bg-[#fdfdfc] dark:bg-[#1a1a1c] select-none" @click="isPanelOpen = false">
        <canvas ref="canvasRef" class="w-full h-full cursor-grab active:cursor-grabbing"></canvas>
        
        <!-- Toggle Button -->
        <button 
            @click.stop="isPanelOpen = !isPanelOpen" 
            class="absolute top-6 right-6 z-20 w-10 h-10 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md rounded-full shadow-lg flex items-center justify-center border border-gray-200 dark:border-[#3a3a3c] hover:bg-gray-50 dark:hover:bg-[#3a3a3c] transition-all"
            :class="{ 'rotate-90': isPanelOpen }"
        >
            <Settings2 class="w-5 h-5 text-gray-700 dark:text-gray-300" />
        </button>
        
        <!-- Obsidian-Style Floating Settings Panel -->
        <div 
            v-show="isPanelOpen"
            @click.stop
            class="absolute top-20 right-6 z-20 w-80 max-h-[calc(100vh-100px)] flex flex-col bg-white/95 dark:bg-[#1e1e20]/95 backdrop-blur-2xl rounded-2xl shadow-2xl border border-gray-200/50 dark:border-[#3a3a3c]/50 overflow-hidden animate-in slide-in-from-right-8 duration-300"
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
                        <input type="range" v-model.number="nodeSize" min="0.5" max="3" step="0.1" class="w-full range-slider" />
                    </div>

                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Link Thickness</label>
                            <span class="text-xs text-gray-500 font-mono">{{ linkThickness.toFixed(1) }}x</span>
                        </div>
                        <input type="range" v-model.number="linkThickness" min="0.5" max="3" step="0.1" class="w-full range-slider" />
                    </div>
                </div>

                <!-- Forces Tab -->
                <div v-show="activeTab === 'forces'" class="space-y-6">
                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Repel Force</label>
                            <span class="text-xs text-gray-500 font-mono">{{ repelForce }}</span>
                        </div>
                        <input type="range" v-model.number="repelForce" min="50" max="400" step="10" class="w-full range-slider" />
                        <p class="text-[11px] text-gray-400">Push nodes further apart</p>
                    </div>

                    <div class="space-y-2">
                        <div class="flex justify-between items-center">
                            <label class="text-sm font-medium text-gray-700 dark:text-gray-200">Link Distance</label>
                            <span class="text-xs text-gray-500 font-mono">{{ linkDist }}</span>
                        </div>
                        <input type="range" v-model.number="linkDist" min="20" max="150" step="5" class="w-full range-slider" />
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
