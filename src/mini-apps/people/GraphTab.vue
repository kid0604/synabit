<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import * as d3 from 'd3';
import { useNodeService } from '../../composables/useNodeService';
import { Share2, Users, X, Edit2, Expand, Shrink } from 'lucide-vue-next';

const props = defineProps<{
    person: any;
    allPeople: any[];
    vaultPath: string;
}>();

const emit = defineEmits<{
    (e: 'select-person', person: any): void;
    (e: 'open-node', id: string, type: string): void;
    (e: 'unlink', personId: string): void;
    (e: 'edit-link', personId: string): void;
}>();
const ns = useNodeService();

// --- Data ---
interface GraphNode extends d3.SimulationNodeDatum {
    id: string;
    label: string;
    type: 'center' | 'person' | 'note' | 'task' | 'quickcap' | 'event';
    relation?: string;
    avatar?: string;
    degree: number;
    radius: number;
}

interface GraphLink extends d3.SimulationLinkDatum<GraphNode> {
    label?: string;
}

const containerRef = ref<HTMLDivElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);
const hoveredNode = ref<GraphNode | null>(null);
const linkedNodes = ref<any[]>([]);
const showSecondDegree = ref(false);

let simulation: d3.Simulation<GraphNode, GraphLink> | null = null;
let zoomBehavior: d3.ZoomBehavior<HTMLCanvasElement, unknown> | null = null;
let transform = d3.zoomIdentity;
let currentNodes: GraphNode[] = [];
let currentLinks: GraphLink[] = [];
let dragSubject: GraphNode | null = null;

const colorMap: Record<string, string> = {
    center: '#8b5cf6',   // violet
    person: '#f97316',   // orange
    note: '#3b82f6',     // blue
    task: '#10b981',     // emerald
    quickcap: '#eab308', // yellow
    event: '#f43f5e',    // rose
};

const RELATION_LABELS: Record<string, string> = {
    friend: '👫', family: '👨‍👩‍👧', colleague: '💼', partner: '❤️',
    mentor: '🎓', mentee: '📚', neighbor: '🏠', introduced_by: '🤝',
    client: '📋', other: '🔗',
};

// --- Load linked activity nodes from backend ---
const loadLinkedNodes = async () => {
    if (!props.person?.title) return;
    try {
        linkedNodes.value = await ns.getLinkedNodes(props.person.title, props.person.id);
    } catch (e) {
        linkedNodes.value = [];
    }
};

// --- Build graph data ---
const buildGraphData = (): { nodes: GraphNode[]; links: GraphLink[] } => {
    const nodes: GraphNode[] = [];
    const links: GraphLink[] = [];
    const nodeMap = new Map<string, GraphNode>();

    // Center node = current person
    const centerNode: GraphNode = {
        id: props.person.id,
        label: props.person.title,
        type: 'center',
        avatar: props.person.properties?.avatar,
        degree: 0,
        radius: 28,
    };
    nodes.push(centerNode);
    nodeMap.set(centerNode.id, centerNode);

    // Person connections
    const connections: Array<{ person_id: string; name: string; relation_type: string }> = props.person?.properties?.connections || [];
    
    // Store 1st degree persons to process their connections later if needed
    const firstDegreeNodes: { id: string, node: GraphNode, data: any }[] = [];
    
    for (const conn of connections) {
        if (nodeMap.has(conn.person_id)) continue;
        const personData = props.allPeople.find(p => p.id === conn.person_id);
        const pNode: GraphNode = {
            id: conn.person_id,
            label: conn.name,
            type: 'person',
            relation: conn.relation_type,
            avatar: personData?.properties?.avatar,
            degree: 0,
            radius: 20,
        };
        nodes.push(pNode);
        nodeMap.set(pNode.id, pNode);
        links.push({ source: centerNode, target: pNode, label: RELATION_LABELS[conn.relation_type] || conn.relation_type });
        centerNode.degree++;
        pNode.degree++;
        
        if (personData) {
            firstDegreeNodes.push({ id: conn.person_id, node: pNode, data: personData });
        }
    }
    
    // 2nd degree connections
    if (showSecondDegree.value) {
        for (const fd of firstDegreeNodes) {
            const fdConnections: Array<{ person_id: string; name: string; relation_type: string }> = fd.data.properties?.connections || [];
            for (const conn of fdConnections) {
                // Don't link back to center node if already linked
                if (conn.person_id === centerNode.id) continue;
                
                let targetNode = nodeMap.get(conn.person_id);
                if (!targetNode) {
                    const personData = props.allPeople.find(p => p.id === conn.person_id);
                    targetNode = {
                        id: conn.person_id,
                        label: conn.name,
                        type: 'person',
                        relation: conn.relation_type,
                        avatar: personData?.properties?.avatar,
                        degree: 0,
                        radius: 16, // slightly smaller for 2nd degree
                    };
                    nodes.push(targetNode);
                    nodeMap.set(targetNode.id, targetNode);
                }
                
                // Add link between 1st degree and 2nd degree
                // Avoid duplicate links
                const linkExists = links.some(l => 
                    ((l.source as any).id === fd.node.id && (l.target as any).id === targetNode!.id) ||
                    ((l.target as any).id === fd.node.id && (l.source as any).id === targetNode!.id)
                );
                
                if (!linkExists) {
                    links.push({ source: fd.node, target: targetNode, label: RELATION_LABELS[conn.relation_type] || conn.relation_type });
                    fd.node.degree++;
                    targetNode.degree++;
                }
            }
        }
    }

    // Linked activity nodes (notes, tasks, etc.)
    for (const node of linkedNodes.value) {
        if (nodeMap.has(node.id)) continue;
        const nType = node.node_type || 'note';
        if (!['note', 'task', 'quickcap', 'event'].includes(nType)) continue;
        const actNode: GraphNode = {
            id: node.id,
            label: node.title,
            type: nType as any,
            degree: 0,
            radius: 12,
        };
        nodes.push(actNode);
        nodeMap.set(actNode.id, actNode);
        links.push({ source: centerNode, target: actNode });
        centerNode.degree++;
        actNode.degree++;
    }

    return { nodes, links };
};

// --- Canvas rendering ---
const renderGraph = () => {
    if (!canvasRef.value || !containerRef.value) return;

    const width = containerRef.value.clientWidth;
    const height = containerRef.value.clientHeight;
    if (width === 0 || height === 0) return;

    const canvas = canvasRef.value;
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const { nodes, links } = buildGraphData();
    currentNodes = nodes;
    currentLinks = links;

    // Stop previous
    if (simulation) simulation.stop();

    // Zoom
    transform = d3.zoomIdentity;
    zoomBehavior = d3.zoom<HTMLCanvasElement, unknown>()
        .scaleExtent([0.3, 3])
        .on('zoom', (event) => {
            transform = event.transform;
            draw(ctx);
        });

    d3.select(canvas)
        .call(zoomBehavior as any)
        .call(zoomBehavior.transform as any, transform)
        .on('dblclick.zoom', null);

    // Simulation
    simulation = d3.forceSimulation<GraphNode>(nodes)
        .force('link', d3.forceLink<GraphNode, GraphLink>(links).id(d => d.id).distance(d => {
            const s = d.source as GraphNode;
            const t = d.target as GraphNode;
            if (s.type === 'center' && t.type === 'person') return 120;
            return 80;
        }))
        .force('charge', d3.forceManyBody().strength(-200).distanceMax(300))
        .force('center', d3.forceCenter(width / 2, height / 2))
        .force('collide', d3.forceCollide<GraphNode>().radius(d => d.radius + 8))
        .on('tick', () => draw(ctx));

    // Interactions
    d3.select(canvas)
        .on('mousemove', (e) => {
            const [x, y] = d3.pointer(e, canvas);
            const invX = transform.invertX(x);
            const invY = transform.invertY(y);
            const found = simulation!.find(invX, invY, 30 / transform.k);
            if (found !== hoveredNode.value) {
                hoveredNode.value = found || null;
                canvas.style.cursor = found ? 'pointer' : 'grab';
                draw(ctx);
            }
        })
        .on('click', () => {
            if (hoveredNode.value && hoveredNode.value.type === 'person') {
                const target = props.allPeople.find(p => p.id === hoveredNode.value!.id);
                if (target) emit('select-person', target);
            }
        });

    // Dragging
    const drag = d3.drag<HTMLCanvasElement, unknown>()
        .subject((e) => {
            const [x, y] = d3.pointer(e, canvas);
            dragSubject = simulation!.find(transform.invertX(x), transform.invertY(y), 30 / transform.k) || null;
            return dragSubject;
        })
        .on('start', (e) => {
            if (!e.active) simulation!.alphaTarget(0.3).restart();
            if (dragSubject) { dragSubject.fx = dragSubject.x; dragSubject.fy = dragSubject.y; }
        })
        .on('drag', (e) => {
            if (dragSubject) { dragSubject.fx = transform.invertX(e.x); dragSubject.fy = transform.invertY(e.y); }
        })
        .on('end', (e) => {
            if (!e.active) simulation!.alphaTarget(0);
            if (dragSubject) { dragSubject.fx = null; dragSubject.fy = null; dragSubject = null; }
        });
    d3.select(canvas).call(drag as any);
};

const draw = (ctx: CanvasRenderingContext2D) => {
    ctx.save();
    ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    ctx.translate(transform.x, transform.y);
    ctx.scale(transform.k, transform.k);

    const hovered = hoveredNode.value;
    const connectedIds = new Set<string>();
    if (hovered) {
        connectedIds.add(hovered.id);
        currentLinks.forEach(l => {
            const s = l.source as GraphNode, t = l.target as GraphNode;
            if (s.id === hovered.id) connectedIds.add(t.id);
            if (t.id === hovered.id) connectedIds.add(s.id);
        });
    }

    // Links
    currentLinks.forEach(link => {
        const s = link.source as GraphNode, t = link.target as GraphNode;
        if (s.x === undefined || s.y === undefined || t.x === undefined || t.y === undefined) return;
        
        const isConnected = !hovered || connectedIds.has(s.id) && connectedIds.has(t.id);
        ctx.beginPath();
        ctx.moveTo(s.x, s.y);
        ctx.lineTo(t.x, t.y);
        const strokeColor = isConnected ? 'rgba(156, 163, 175, 0.6)' : 'rgba(156, 163, 175, 0.1)';
        ctx.strokeStyle = strokeColor;
        ctx.lineWidth = isConnected && (s.type === 'person' || t.type === 'person') ? 2 : 1;
        ctx.stroke();

        // Draw arrowhead pointing from target to source (center person)
        if (link.label && isConnected && t.type === 'person') {
            const dx = t.x - s.x;
            const dy = t.y - s.y;
            const dist = Math.sqrt(dx * dx + dy * dy);
            if (dist > 0) {
                // Pointing AT source (s)
                const tipX = s.x + (dx / dist) * (s.radius + 2);
                const tipY = s.y + (dy / dist) * (s.radius + 2);
                const angle = Math.atan2(-dy, -dx);
                const arrowLength = 7;
                
                ctx.beginPath();
                ctx.moveTo(tipX, tipY);
                ctx.lineTo(tipX - arrowLength * Math.cos(angle - Math.PI / 7), tipY - arrowLength * Math.sin(angle - Math.PI / 7));
                ctx.lineTo(tipX - arrowLength * Math.cos(angle + Math.PI / 7), tipY - arrowLength * Math.sin(angle + Math.PI / 7));
                ctx.closePath();
                ctx.fillStyle = strokeColor;
                ctx.fill();
            }
        }

        // Link label (text) for person connections
        if (link.label && isConnected) {
            const mx = (s.x + t.x) / 2, my = (s.y + t.y) / 2;
            ctx.font = '10px Inter, sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillStyle = 'rgba(100, 100, 100, 0.9)';
            ctx.fillText(link.label, mx, my - 8);
        }
    });

    // Nodes
    currentNodes.forEach(node => {
        if (node.x === undefined || node.y === undefined) return;
        
        const isConnected = !hovered || connectedIds.has(node.id);
        const alpha = isConnected ? 1.0 : 0.2;
        const r = node.radius;

        ctx.globalAlpha = alpha;

        if (node.type === 'center') {
            // Gradient ring for center node
            ctx.beginPath();
            ctx.arc(node.x, node.y, r + 3, 0, Math.PI * 2);
            const grad = ctx.createRadialGradient(node.x, node.y, r - 2, node.x, node.y, r + 4);
            grad.addColorStop(0, 'rgba(139, 92, 246, 0.4)');
            grad.addColorStop(1, 'rgba(139, 92, 246, 0)');
            ctx.fillStyle = grad;
            ctx.fill();
        }

        ctx.beginPath();
        ctx.arc(node.x, node.y, r, 0, Math.PI * 2);
        ctx.fillStyle = colorMap[node.type] || '#9ca3af';
        ctx.fill();
        ctx.lineWidth = node === hovered ? 3 : 1.5;
        ctx.strokeStyle = node === hovered ? '#fff' : 'rgba(255,255,255,0.5)';
        ctx.stroke();

        // Label
        if (node === hovered || node.type === 'center' || node.type === 'person') {
            ctx.font = node.type === 'center' ? 'bold 13px Inter, sans-serif' : '11px Inter, sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillStyle = isConnected ? 'rgba(100,100,100,0.9)' : 'rgba(100,100,100,0.3)';
            ctx.fillText(node.label, node.x, node.y + r + 5, 120);
        }

        // Type icon text for activity nodes
        if (node.type !== 'center' && node.type !== 'person') {
            ctx.font = '10px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillStyle = '#fff';
            const icon = node.type === 'note' ? '📝' : node.type === 'task' ? '✓' : node.type === 'event' ? '📅' : '⚡';
            ctx.fillText(icon, node.x, node.y);
        }

        ctx.globalAlpha = 1.0;
    });

    ctx.restore();
};

// --- Connections list ---
const connections = computed(() => {
    return (props.person?.properties?.connections || []) as Array<{ person_id: string; name: string; relation_type: string }>;
});


// --- Lifecycle ---
let cleanupObserver: (() => void) | null = null;

const initGraph = async () => {
    await loadLinkedNodes();
    await nextTick();
    if (cleanupObserver) cleanupObserver();
    
    if (!containerRef.value) return;
    
    const observer = new ResizeObserver(entries => {
        const { width: w, height: h } = entries[0].contentRect;
        if (w > 0 && h > 0) {
            if (!simulation) {
                renderGraph();
            } else {
                if (canvasRef.value) {
                    canvasRef.value.width = w;
                    canvasRef.value.height = h;
                }
                simulation.force('center', d3.forceCenter(w / 2, h / 2));
                simulation.alpha(0.3).restart();
            }
        }
    });
    
    observer.observe(containerRef.value);
    cleanupObserver = () => observer.disconnect();
};

watch([() => props.person?.id, () => props.person?.properties?.connections?.length], () => {
    if (simulation) {
        simulation.stop();
        simulation = null;
    }
    initGraph();
}, { immediate: false });

watch(() => showSecondDegree.value, () => {
    if (simulation) {
        simulation.stop();
        simulation = null;
    }
    initGraph();
});

onMounted(() => {
    setTimeout(() => initGraph(), 150);
});

onUnmounted(() => {
    if (simulation) simulation.stop();
    if (cleanupObserver) cleanupObserver();
});
</script>

<template>
    <div class="h-full flex flex-col">
        <!-- Connections Summary -->
        <div v-if="connections.length > 0" class="flex-shrink-0 px-1 pb-3">
            <h3 class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2 flex items-center gap-1.5">
                <Users class="w-3.5 h-3.5" /> Connections ({{ connections.length }})
            </h3>
            <div class="flex flex-wrap gap-1.5">
                <div v-for="conn in connections" :key="conn.person_id"
                    class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-orange-50 dark:bg-orange-900/10 border border-orange-200 dark:border-orange-900/20 group cursor-pointer hover:bg-orange-100 dark:hover:bg-orange-900/20 transition-colors"
                    @click="() => { const p = allPeople.find(pp => pp.id === conn.person_id); if (p) emit('select-person', p); }"
                >
                    <span class="text-xs">{{ RELATION_LABELS[conn.relation_type] || '🔗' }}</span>
                    <span class="text-xs font-medium text-gray-700 dark:text-gray-300">{{ conn.name }}</span>
                    <button @click.stop="emit('edit-link', conn.person_id)"
                        class="p-0.5 opacity-0 group-hover:opacity-100 text-blue-400 hover:text-blue-600 transition-all" title="Edit link">
                        <Edit2 class="w-3 h-3" />
                    </button>
                    <button @click.stop="emit('unlink', conn.person_id)"
                        class="p-0.5 opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 transition-all" title="Remove link">
                        <X class="w-3 h-3" />
                    </button>
                </div>
            </div>
        </div>

        <!-- Graph Canvas -->
        <div ref="containerRef" class="flex-1 min-h-0 relative rounded-xl overflow-hidden bg-gray-50 dark:bg-[#1a1a1c] border border-border dark:border-border-dark">
            <canvas ref="canvasRef" class="w-full h-full cursor-grab active:cursor-grabbing"></canvas>

            <!-- Empty state overlay -->
            <div v-if="connections.length === 0 && linkedNodes.length === 0" class="absolute inset-0 flex flex-col items-center justify-center bg-gray-50 dark:bg-[#1a1a1c] z-10">
                <div class="w-16 h-16 rounded-full bg-gradient-to-br from-purple-100 to-blue-100 dark:from-purple-900/20 dark:to-blue-900/20 flex items-center justify-center mb-4">
                    <Share2 class="w-7 h-7 text-purple-400 dark:text-purple-500" />
                </div>
                <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 mb-1">{{ $t('people.no_connections') }}</h3>
                <p class="text-xs text-gray-400 dark:text-gray-500 text-center max-w-[200px]">
                    Use the <strong>Link Person</strong> button above to connect contacts and build your relationship map.
                </p>
            </div>

            <!-- Legend and Controls -->
            <div v-if="connections.length > 0 || linkedNodes.length > 0" class="absolute bottom-3 left-3 flex flex-col gap-2 pointer-events-none">
                
                <!-- Expand/Shrink Toggle -->
                <button 
                    @click="showSecondDegree = !showSecondDegree"
                    class="pointer-events-auto self-start flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md border border-gray-200 dark:border-gray-700 text-xs font-medium text-gray-700 dark:text-gray-300 hover:bg-white dark:hover:bg-[#2a2a2d] transition-colors shadow-sm"
                >
                    <Expand v-if="!showSecondDegree" class="w-3.5 h-3.5" />
                    <Shrink v-else class="w-3.5 h-3.5" />
                    {{ showSecondDegree ? 'Show Less' : 'Show More' }}
                </button>

                <!-- Legend -->
                <div class="flex items-center gap-3 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md rounded-lg px-3 py-1.5 border border-gray-200 dark:border-gray-700 text-[10px] shadow-sm">
                    <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-violet-500"></span> Current</span>
                    <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-orange-500"></span> People</span>
                    <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-blue-500"></span> Notes</span>
                    <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-emerald-500"></span> Tasks</span>
                </div>
            </div>
        </div>
    </div>
</template>
