<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from 'vue';
import { NodeViewWrapper } from '@tiptap/vue-3';
import {
  MapPin, ExternalLink, RefreshCw, Pencil, Trash2, Check, X,
  AlignLeft, AlignCenter, AlignRight, Navigation
} from 'lucide-vue-next';
import L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { openUrl } from '@tauri-apps/plugin-opener';

const props = defineProps<{
  node: any;
  updateAttributes: (attrs: Record<string, any>) => void;
  deleteNode: () => void;
  getPos: () => number;
  editor: any;
  selected: boolean;
}>();

const mapContainer = ref<HTMLElement | null>(null);
const blockRef = ref<HTMLElement | null>(null);
let leafletMap: L.Map | null = null;
let marker: L.Marker | null = null;

const isRoute = computed(() => props.node.attrs.mode === 'route');
const provider = computed(() => props.node.attrs.provider || 'osm');
const isGoogle = computed(() => provider.value === 'google');
const isOsmRoute = computed(() => isRoute.value && provider.value === 'osm');
const isGoogleRoute = computed(() => isRoute.value && provider.value === 'google');
const blockWidth = computed(() => props.node.attrs.width || '100%');
const blockHeight = computed(() => props.node.attrs.height || '200px');
const blockAlign = computed(() => props.node.attrs.align || 'center');

const alignStyle = computed(() => {
  switch (blockAlign.value) {
    case 'left': return { marginRight: 'auto', marginLeft: '0' };
    case 'right': return { marginLeft: 'auto', marginRight: '0' };
    default: return { marginLeft: 'auto', marginRight: 'auto' };
  }
});

// --- Edit mode ---
const editing = ref(false);
const editLabel = ref('');
const editLat = ref('');
const editLng = ref('');

const startEdit = () => {
  editLabel.value = props.node.attrs.label || '';
  editLat.value = String(props.node.attrs.lat);
  editLng.value = String(props.node.attrs.lng);
  editing.value = true;
};

const cancelEdit = () => { editing.value = false; };

const saveEdit = async () => {
  const lat = parseFloat(editLat.value);
  const lng = parseFloat(editLng.value);
  if (isNaN(lat) || isNaN(lng) || lat < -90 || lat > 90 || lng < -180 || lng > 180) return;
  props.updateAttributes({ lat, lng, label: editLabel.value });
  editing.value = false;
  if (!isGoogle.value && leafletMap) {
    leafletMap.setView([lat, lng], props.node.attrs.zoom || 15);
    if (marker) marker.setLatLng([lat, lng]);
  }
};

// --- Resize ---
const resizing = ref(false);

const onResizeWidth = (e: MouseEvent, side: 'left' | 'right') => {
  e.preventDefault();
  e.stopPropagation();
  resizing.value = true;

  const startX = e.clientX;
  const container = blockRef.value;
  if (!container) return;

  const parentWidth = container.parentElement?.clientWidth || container.clientWidth;
  const startW = container.clientWidth;

  const onMove = (ev: MouseEvent) => {
    const dx = side === 'right' ? ev.clientX - startX : startX - ev.clientX;
    const factor = blockAlign.value === 'center' ? 2 : 1;
    const newW = Math.max(200, Math.min(parentWidth, startW + dx * factor));
    const pct = Math.round((newW / parentWidth) * 100);
    props.updateAttributes({ width: `${pct}%` });
  };

  const onUp = () => {
    resizing.value = false;
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
    if (leafletMap) setTimeout(() => leafletMap?.invalidateSize(), 50);
  };

  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
};

const onResizeHeight = (e: MouseEvent) => {
  e.preventDefault();
  e.stopPropagation();
  resizing.value = true;

  const startY = e.clientY;
  const mapEl = mapContainer.value || blockRef.value?.querySelector('.location-map') as HTMLElement;
  if (!mapEl) return;
  const startH = mapEl.clientHeight;

  const onMove = (ev: MouseEvent) => {
    const dy = ev.clientY - startY;
    const newH = Math.max(120, Math.min(600, startH + dy));
    props.updateAttributes({ height: `${newH}px` });
  };

  const onUp = () => {
    resizing.value = false;
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
    if (leafletMap) setTimeout(() => leafletMap?.invalidateSize(), 50);
  };

  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
};

// --- Alignment ---
const setAlign = (align: 'left' | 'center' | 'right') => {
  props.updateAttributes({ align });
};

// Programmatically select this node (for OSM overlay click)
const selectNode = () => {
  const pos = props.getPos();
  if (pos != null && props.editor) {
    props.editor.commands.setNodeSelection(pos);
  }
};

// Embed URL for iframe
const googleEmbedUrl = computed(() => {
  const a = props.node.attrs;
  if (isRoute.value && a.routeUrl) {
    return buildRouteEmbedUrl(a.routeUrl);
  }
  const q = a.label ? encodeURIComponent(a.label) : `${a.lat},${a.lng}`;
  return `https://maps.google.com/maps?q=${q}&z=${a.zoom || 15}&output=embed&t=m`;
});

/** Convert a Google Maps directions URL → embeddable iframe URL */
function buildRouteEmbedUrl(url: string): string {
  try {
    const u = new URL(url);
    // Extract place parts from /maps/dir/Place1/Place2/@...
    const allParts = u.pathname.replace(/^\/maps\/dir\/?/, '').split('/').filter(Boolean);
    const parts = allParts.filter(p => !p.startsWith('@') && !p.startsWith('data'));
    if (parts.length >= 2) {
      const origin = decodeURIComponent(parts[0]).replace(/\+/g, ' ');
      const dest = decodeURIComponent(parts[1]).replace(/\+/g, ' ');
      return `https://maps.google.com/maps?saddr=${encodeURIComponent(origin)}&daddr=${encodeURIComponent(dest)}&output=embed`;
    }
    // Fallback: try api params
    const apiOrigin = u.searchParams.get('origin');
    const apiDest = u.searchParams.get('destination');
    if (apiOrigin && apiDest) {
      return `https://maps.google.com/maps?saddr=${encodeURIComponent(apiOrigin)}&daddr=${encodeURIComponent(apiDest)}&output=embed`;
    }
  } catch { /* fallback */ }
  // Last resort: just append output=embed
  return url.includes('output=embed') ? url : url + '&output=embed';
}

// Custom marker icons
const pinIcon = L.divIcon({
  html: `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="#ef4444" stroke="#ffffff" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 10c0 4.993-5.539 10.193-7.399 11.799a1 1 0 0 1-1.202 0C9.539 20.193 4 14.993 4 10a8 8 0 0 1 16 0"/><circle cx="12" cy="10" r="3"/></svg>`,
  className: 'location-pin-icon',
  iconSize: [28, 28],
  iconAnchor: [14, 28],
  popupAnchor: [0, -28],
});

const originIcon = L.divIcon({
  html: `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="#22c55e" stroke="#fff" stroke-width="1.5"><path d="M20 10c0 4.993-5.539 10.193-7.399 11.799a1 1 0 0 1-1.202 0C9.539 20.193 4 14.993 4 10a8 8 0 0 1 16 0"/><circle cx="12" cy="10" r="3"/></svg>`,
  className: 'location-pin-icon', iconSize: [28, 28], iconAnchor: [14, 28], popupAnchor: [0, -28],
});
const destIcon = L.divIcon({
  html: `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="#ef4444" stroke="#fff" stroke-width="1.5"><path d="M20 10c0 4.993-5.539 10.193-7.399 11.799a1 1 0 0 1-1.202 0C9.539 20.193 4 14.993 4 10a8 8 0 0 1 16 0"/><circle cx="12" cy="10" r="3"/></svg>`,
  className: 'location-pin-icon', iconSize: [28, 28], iconAnchor: [14, 28], popupAnchor: [0, -28],
});

/** Parse OSM route coords from URL: ?route=lat1,lng1;lat2,lng2 */
function parseOsmRouteCoords(url: string): { oLat: number; oLng: number; dLat: number; dLng: number } | null {
  try {
    const u = new URL(url);
    const route = u.searchParams.get('route');
    if (!route) return null;
    const pts = route.split(';').map(p => p.split(',').map(Number));
    if (pts.length >= 2 && pts[0].length >= 2 && pts[pts.length-1].length >= 2) {
      return { oLat: pts[0][0], oLng: pts[0][1], dLat: pts[pts.length-1][0], dLng: pts[pts.length-1][1] };
    }
  } catch { /* */ }
  return null;
}

let routeLine: L.GeoJSON | null = null;
let originMarker: L.Marker | null = null;
let destMarker: L.Marker | null = null;

/** Fetch OSRM route and draw polyline */
const fetchOsrmRoute = async (oLat: number, oLng: number, dLat: number, dLng: number) => {
  if (!leafletMap) return;
  try {
    const res = await fetch(`https://router.project-osrm.org/route/v1/driving/${oLng},${oLat};${dLng},${dLat}?overview=full&geometries=geojson`);
    const data = await res.json();
    if (data.routes?.[0]) {
      if (routeLine) { leafletMap.removeLayer(routeLine); routeLine = null; }
      routeLine = L.geoJSON(data.routes[0].geometry, {
        style: { color: '#6366f1', weight: 4, opacity: 0.8 },
      }).addTo(leafletMap);
      leafletMap.fitBounds(routeLine.getBounds().pad(0.15));
    }
  } catch (e) { console.warn('OSRM route fetch failed:', e); }
};

const initLeafletMap = () => {
  // Google routes use iframe; OSM routes use Leaflet
  if (!mapContainer.value || (isGoogle.value && !isOsmRoute.value)) return;
  if (isGoogleRoute.value) return; // Google route = iframe only
  const a = props.node.attrs;

  const center: [number, number] = isOsmRoute.value
    ? (() => { const c = parseOsmRouteCoords(a.routeUrl); return c ? [(c.oLat+c.dLat)/2, (c.oLng+c.dLng)/2] as [number, number] : [21, 105]; })()
    : [a.lat, a.lng];

  leafletMap = L.map(mapContainer.value, {
    center, zoom: a.zoom || 13,
    zoomControl: false, attributionControl: false,
    scrollWheelZoom: false, dragging: false,
    touchZoom: false, doubleClickZoom: false,
    boxZoom: false, keyboard: false,
  });

  L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', { maxZoom: 19 }).addTo(leafletMap);
  L.control.attribution({ position: 'bottomright', prefix: false })
    .addAttribution('\u00a9 <a href="https://openstreetmap.org" target="_blank" rel="noopener">OSM</a>')
    .addTo(leafletMap);

  if (isOsmRoute.value) {
    const coords = parseOsmRouteCoords(a.routeUrl);
    if (coords) {
      originMarker = L.marker([coords.oLat, coords.oLng], { icon: originIcon }).addTo(leafletMap);
      destMarker = L.marker([coords.dLat, coords.dLng], { icon: destIcon }).addTo(leafletMap);
      const bounds = L.latLngBounds([coords.oLat, coords.oLng], [coords.dLat, coords.dLng]);
      leafletMap.fitBounds(bounds.pad(0.2));
      setTimeout(() => { leafletMap?.invalidateSize(); fetchOsrmRoute(coords.oLat, coords.oLng, coords.dLat, coords.dLng); }, 200);
    }
  } else {
    // Pin mode
    marker = L.marker([a.lat, a.lng], { icon: pinIcon }).addTo(leafletMap);
    if (a.label) marker.bindPopup(`<b>${a.label}</b>`, { closeButton: false, className: 'location-popup' });
  }

  setTimeout(() => leafletMap?.invalidateSize(), 100);
  setTimeout(() => leafletMap?.invalidateSize(), 300);
};

const destroyLeafletMap = () => {
  if (leafletMap) { leafletMap.remove(); leafletMap = null; marker = null; originMarker = null; destMarker = null; routeLine = null; }
};

const toggleProvider = () => {
  if (isRoute.value) return;
  props.updateAttributes({ provider: isGoogle.value ? 'osm' : 'google' });
};

const openExternal = () => {
  const a = props.node.attrs;
  if (isRoute.value && a.routeUrl) {
    openUrl(a.routeUrl);
  } else if (isGoogle.value) {
    openUrl(`https://www.google.com/maps?q=${a.lat},${a.lng}`);
  } else {
    openUrl(`https://www.openstreetmap.org/?mlat=${a.lat}&mlon=${a.lng}#map=${a.zoom || 15}/${a.lat}/${a.lng}`);
  }
};

onMounted(() => {
  if (isOsmRoute.value || (!isGoogle.value && !isRoute.value)) {
    initLeafletMap();
  }
});
onBeforeUnmount(() => destroyLeafletMap());

watch(() => props.node.attrs.provider, async (p) => {
  if (p === 'google') destroyLeafletMap();
  else { await nextTick(); initLeafletMap(); }
});

watch(() => [props.node.attrs.lat, props.node.attrs.lng, props.node.attrs.zoom], () => {
  if (leafletMap && !isGoogle.value) {
    leafletMap.setView([props.node.attrs.lat, props.node.attrs.lng], props.node.attrs.zoom);
    if (marker) marker.setLatLng([props.node.attrs.lat, props.node.attrs.lng]);
  }
});

// Invalidate leaflet on width change
watch(() => props.node.attrs.width, () => {
  if (leafletMap) nextTick(() => leafletMap?.invalidateSize());
});

watch(() => props.selected, (sel) => { if (!sel) editing.value = false; });
</script>

<template>
  <NodeViewWrapper
    class="location-node-wrapper"
    :class="[
      { 'is-selected': selected, 'is-resizing': resizing },
      `loc-align-${blockAlign}`
    ]"
  >
    <div
      ref="blockRef"
      class="location-block-container"
      :style="{ width: blockWidth, ...alignStyle }"
    >
      <!-- Resize handle LEFT -->
      <div
        v-if="selected"
        class="loc-resize-handle loc-resize-left"
        @mousedown="(e: MouseEvent) => onResizeWidth(e, 'left')"
      >
        <div class="loc-resize-bar" />
      </div>

      <!-- Resize handle RIGHT -->
      <div
        v-if="selected"
        class="loc-resize-handle loc-resize-right"
        @mousedown="(e: MouseEvent) => onResizeWidth(e, 'right')"
      >
        <div class="loc-resize-bar" />
      </div>

      <!-- Resize handle BOTTOM -->
      <div
        v-if="selected"
        class="loc-resize-handle loc-resize-bottom"
        @mousedown="onResizeHeight"
      >
        <div class="loc-resize-bar-h" />
      </div>

      <!-- Map -->
      <!-- Leaflet: pin mode (OSM provider) OR OSM route -->
      <div v-if="(!isGoogle && !isRoute) || isOsmRoute" class="location-map-wrap" :style="{ height: blockHeight }">
        <div ref="mapContainer" class="location-map" style="height: 100%" />
        <div class="location-map-overlay" @click="selectNode" />
      </div>
      <!-- iframe: pin mode (Google provider) OR Google route -->
      <div v-else class="location-map" :style="{ height: blockHeight }">
        <iframe
          :src="googleEmbedUrl"
          class="w-full h-full border-0"
          loading="lazy"
          referrerpolicy="no-referrer-when-downgrade"
          allowfullscreen
        />
      </div>

      <!-- Bubble toolbar -->
      <Transition name="loc-bubble">
        <div v-if="selected && !editing" class="location-bubble" @mousedown.prevent>
          <button v-if="!isRoute" @click="startEdit" title="Edit" class="loc-bubble-btn">
            <Pencil class="w-3.5 h-3.5" />
          </button>
          <div v-if="!isRoute" class="loc-bubble-sep" />
          <!-- Alignment -->
          <button @click="setAlign('left')" title="Align left" class="loc-bubble-btn" :class="{ 'loc-bubble-active': blockAlign === 'left' }">
            <AlignLeft class="w-3.5 h-3.5" />
          </button>
          <button @click="setAlign('center')" title="Align center" class="loc-bubble-btn" :class="{ 'loc-bubble-active': blockAlign === 'center' }">
            <AlignCenter class="w-3.5 h-3.5" />
          </button>
          <button @click="setAlign('right')" title="Align right" class="loc-bubble-btn" :class="{ 'loc-bubble-active': blockAlign === 'right' }">
            <AlignRight class="w-3.5 h-3.5" />
          </button>
          <div class="loc-bubble-sep" />
          <button v-if="!isRoute" @click="toggleProvider" :title="`Switch to ${isGoogle ? 'OSM' : 'Google Maps'}`" class="loc-bubble-btn">
            <RefreshCw class="w-3.5 h-3.5" />
          </button>
          <button @click="openExternal" title="Open in browser" class="loc-bubble-btn">
            <ExternalLink class="w-3.5 h-3.5" />
          </button>
          <div class="loc-bubble-sep" />
          <button @click="deleteNode" title="Remove" class="loc-bubble-btn loc-bubble-danger">
            <Trash2 class="w-3.5 h-3.5" />
          </button>
        </div>
      </Transition>

      <!-- Edit panel -->
      <Transition name="loc-bubble">
        <div v-if="editing" class="location-edit-panel" @keydown.stop @mousedown.stop>
          <div class="loc-edit-row">
            <label class="loc-edit-label">Label</label>
            <input v-model="editLabel" type="text" placeholder="e.g., Hanoi Opera House" class="loc-edit-input" @keydown.enter.stop="saveEdit" />
          </div>
          <div class="loc-edit-row loc-edit-coords">
            <div class="loc-edit-coord">
              <label class="loc-edit-label">Lat</label>
              <input v-model="editLat" type="text" placeholder="21.0285" class="loc-edit-input" @keydown.enter.stop="saveEdit" />
            </div>
            <div class="loc-edit-coord">
              <label class="loc-edit-label">Lng</label>
              <input v-model="editLng" type="text" placeholder="105.8542" class="loc-edit-input" @keydown.enter.stop="saveEdit" />
            </div>
          </div>
          <div class="loc-edit-actions">
            <button @click="cancelEdit" class="loc-edit-cancel"><X class="w-3.5 h-3.5" /> Cancel</button>
            <button @click="saveEdit" class="loc-edit-save"><Check class="w-3.5 h-3.5" /> Save</button>
          </div>
        </div>
      </Transition>

      <!-- Info bar -->
      <div class="location-info">
        <div v-if="isRoute" class="location-label" style="gap: 6px;">
          <Navigation class="w-3.5 h-3.5 text-indigo-500 flex-shrink-0" />
          <span class="location-name">{{ node.attrs.label || 'Directions' }}</span>
        </div>
        <div v-else class="location-label">
          <MapPin class="w-3.5 h-3.5 text-red-500 flex-shrink-0" />
          <span class="location-name" :title="`${node.attrs.lat}, ${node.attrs.lng}`">
            {{ node.attrs.label || `${node.attrs.lat.toFixed(5)}, ${node.attrs.lng.toFixed(5)}` }}
          </span>
        </div>
        <span class="location-provider-badge">{{ isRoute ? (isOsmRoute ? 'OpenStreetMap' : 'Google Maps') : (isGoogle ? 'Google' : 'OSM') }}</span>
      </div>
    </div>
  </NodeViewWrapper>
</template>

<style>
.location-pin-icon {
  background: transparent !important;
  border: none !important;
}

.location-node-wrapper {
  margin: 12px 0;
}

.location-block-container {
  border-radius: 12px;
  overflow: visible;
  border: 1px solid #e5e7eb;
  background: #fff;
  transition: box-shadow 0.2s, border-color 0.2s;
  position: relative;
}

.location-node-wrapper.is-selected .location-block-container {
  border-color: #3b82f6;
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15);
}

.dark .location-block-container {
  border-color: #333;
  background: #1e1e1e;
}

.dark .location-node-wrapper.is-selected .location-block-container {
  border-color: #60a5fa;
  box-shadow: 0 0 0 2px rgba(96, 165, 250, 0.15);
}

/* Disable user-select during resize */
.location-node-wrapper.is-resizing * {
  user-select: none !important;
  pointer-events: none !important;
}
.location-node-wrapper.is-resizing .loc-resize-handle {
  pointer-events: auto !important;
}

/* Map */
.location-map-wrap {
  position: relative;
  border-radius: 12px 12px 0 0;
  overflow: hidden;
  isolation: isolate; /* Contains Leaflet's high z-index panes within this stacking context */
}

.location-map {
  width: 100%;
  min-height: 120px;
  border-radius: 12px 12px 0 0;
  overflow: hidden;
}

.location-map-overlay {
  position: absolute;
  inset: 0;
  z-index: 1000; /* Must be above Leaflet's highest pane (~800) */
  cursor: default;
}

/* === Resize Handles === */
.loc-resize-handle {
  position: absolute;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.15s;
}

.location-node-wrapper.is-selected .loc-resize-handle {
  opacity: 1;
}

/* Vertical handles (left/right) */
.loc-resize-left,
.loc-resize-right {
  top: 0;
  bottom: 0;
  width: 16px;
  cursor: col-resize;
}

.loc-resize-left { left: -8px; }
.loc-resize-right { right: -8px; }

.loc-resize-bar {
  width: 4px;
  height: 40px;
  max-height: 40%;
  border-radius: 2px;
  background: #3b82f6;
  opacity: 0.5;
  transition: opacity 0.15s, height 0.15s;
}

.loc-resize-handle:hover .loc-resize-bar {
  opacity: 1;
  height: 48px;
}

/* Horizontal handle (bottom) */
.loc-resize-bottom {
  left: 0;
  right: 0;
  bottom: 28px; /* above the info bar */
  height: 16px;
  cursor: row-resize;
}

.loc-resize-bar-h {
  width: 40px;
  max-width: 30%;
  height: 4px;
  border-radius: 2px;
  background: #3b82f6;
  opacity: 0.5;
  transition: opacity 0.15s, width 0.15s;
}

.loc-resize-handle:hover .loc-resize-bar-h {
  opacity: 1;
  width: 56px;
}

.dark .loc-resize-bar,
.dark .loc-resize-bar-h {
  background: #60a5fa;
}

/* === Bubble Toolbar === */
.location-bubble {
  position: absolute;
  top: 8px;
  left: 0;
  right: 0;
  width: fit-content;
  margin: 0 auto;
  z-index: 50;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px;
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.1), 0 1px 3px rgba(0,0,0,0.06);
  white-space: nowrap;
  -webkit-font-smoothing: subpixel-antialiased;
}

.dark .location-bubble {
  background: #1e1e1e;
  border-color: #333;
  box-shadow: 0 4px 12px rgba(0,0,0,0.4);
}

.loc-bubble-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 7px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: #6b7280;
  font-size: 11px;
  font-weight: 500;
  transition: all 0.12s;
  white-space: nowrap;
}

.loc-bubble-btn:hover {
  background: #f3f4f6;
  color: #111;
}

.loc-bubble-btn.loc-bubble-active {
  background: #111;
  color: #fff;
}

.dark .loc-bubble-btn {
  color: #a1a1aa;
}

.dark .loc-bubble-btn:hover {
  background: #2a2a2a;
  color: #f4f4f5;
}

.dark .loc-bubble-btn.loc-bubble-active {
  background: #f4f4f5;
  color: #111;
}

.loc-bubble-danger:hover {
  background: #fee2e2 !important;
  color: #dc2626 !important;
}

.dark .loc-bubble-danger:hover {
  background: #450a0a !important;
  color: #f87171 !important;
}

.loc-bubble-sep {
  width: 1px;
  height: 18px;
  background: #e5e7eb;
  margin: 0 2px;
}

.dark .loc-bubble-sep {
  background: #3a3a3a;
}

/* Bubble transition */
.loc-bubble-enter-active { transition: opacity 0.15s ease, transform 0.15s ease; }
.loc-bubble-leave-active { transition: opacity 0.1s ease; }
.loc-bubble-enter-from { opacity: 0; transform: translateY(-4px); }
.loc-bubble-leave-to { opacity: 0; }

/* === Edit Panel === */
.location-edit-panel {
  position: absolute;
  top: 8px;
  right: 8px;
  left: 8px;
  z-index: 50;
  padding: 12px;
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.12), 0 2px 6px rgba(0,0,0,0.06);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.dark .location-edit-panel {
  background: #1e1e1e;
  border-color: #333;
  box-shadow: 0 8px 24px rgba(0,0,0,0.5);
}

.loc-edit-row { display: flex; flex-direction: column; gap: 3px; }
.loc-edit-coords { flex-direction: row; gap: 8px; }
.loc-edit-coord { flex: 1; display: flex; flex-direction: column; gap: 3px; }

.loc-edit-label {
  font-size: 10px;
  font-weight: 600;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.dark .loc-edit-label { color: #71717a; }

.loc-edit-input {
  width: 100%;
  padding: 6px 8px;
  border: 1px solid #e0e0e0;
  border-radius: 6px;
  background: #fafafa;
  color: #1c1c1e;
  font-size: 12px;
  outline: none;
  transition: border-color 0.15s;
}

.loc-edit-input:focus { border-color: #3b82f6; }

.dark .loc-edit-input {
  background: #252525;
  border-color: #444;
  color: #f4f4f5;
}

.dark .loc-edit-input:focus { border-color: #60a5fa; }

.loc-edit-actions { display: flex; justify-content: flex-end; gap: 6px; margin-top: 2px; }

.loc-edit-cancel,
.loc-edit-save {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 5px 10px;
  border: none;
  border-radius: 6px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.12s;
}

.loc-edit-cancel { background: #f3f4f6; color: #6b7280; }
.loc-edit-cancel:hover { background: #e5e7eb; color: #374151; }
.dark .loc-edit-cancel { background: #252525; color: #a1a1aa; }
.dark .loc-edit-cancel:hover { background: #333; color: #f4f4f5; }

.loc-edit-save { background: #3b82f6; color: #fff; }
.loc-edit-save:hover { background: #2563eb; }

/* Info bar */
.location-info {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-top: 1px solid #f3f4f6;
  gap: 8px;
}

.dark .location-info { border-top-color: #2a2a2a; }

.location-label {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1;
}

.location-name {
  font-size: 13px;
  font-weight: 500;
  color: #374151;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.dark .location-name { color: #d4d4d8; }

.location-provider-badge {
  font-size: 10px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
  background: #f3f4f6;
  color: #9ca3af;
  flex-shrink: 0;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.dark .location-provider-badge { background: #252525; color: #71717a; }

/* Leaflet popup override */
.location-popup .leaflet-popup-content-wrapper { border-radius: 8px; box-shadow: 0 2px 8px rgba(0,0,0,0.15); font-size: 13px; }
.location-popup .leaflet-popup-content { margin: 8px 12px; }
.location-popup .leaflet-popup-tip { box-shadow: none; }

/* Route info badges */
.route-info-badge {
  font-size: 10px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
  background: #eef2ff;
  color: #6366f1;
  flex-shrink: 0;
  white-space: nowrap;
}
.dark .route-info-badge {
  background: #1e1b4b;
  color: #a5b4fc;
}
</style>
