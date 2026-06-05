<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import * as d3 from 'd3';
import { formatCurrency } from '../currency';

const props = defineProps<{
  data: { label: string; value: number }[];
  title?: string;
  total?: number;
}>();

const chartContainer = ref<HTMLElement | null>(null);

// Curated modern color palette for finance categories
const colorPalette = [
  '#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6',
  '#ec4899', '#14b8a6', '#f97316', '#6366f1', '#06b6d4'
];

const drawChart = () => {
  if (!chartContainer.value || !props.data.length) return;
  
  // Clear previous chart
  d3.select(chartContainer.value).selectAll('*').remove();
  
  const width = chartContainer.value.clientWidth || 300;
  const height = chartContainer.value.clientHeight || 300;
  const margin = 20;
  const radius = Math.min(width, height) / 2 - margin;
  
  const svg = d3.select(chartContainer.value)
    .append('svg')
    .attr('width', width)
    .attr('height', height)
    .append('g')
    .attr('transform', `translate(${width / 2},${height / 2})`);
    
  const color = d3.scaleOrdinal<string>()
    .domain(props.data.map(d => d.label))
    .range(colorPalette);
    
  const pie = d3.pie<{label: string, value: number}>()
    .value(d => d.value)
    .sort(null);
    
  const dataReady = pie(props.data);
  
  // Arc generator for Donut Chart
  const arcGenerator = d3.arc<d3.PieArcDatum<{label: string, value: number}>>()
    .innerRadius(radius * 0.6)         // This makes it a donut chart
    .outerRadius(radius)
    .cornerRadius(4)
    .padAngle(0.02);
    
  // Arc for hover effect
  const arcHover = d3.arc<d3.PieArcDatum<{label: string, value: number}>>()
    .innerRadius(radius * 0.6)
    .outerRadius(radius * 1.05)
    .cornerRadius(4)
    .padAngle(0.02);

  // Build the pie chart
  const slices = svg
    .selectAll('path')
    .data(dataReady)
    .enter()
    .append('path')
    .attr('d', arcGenerator as any)
    .attr('fill', d => color(d.data.label))
    .attr('stroke', 'none')
    .style('opacity', 0)
    .style('cursor', 'pointer');
    
  // Animation
  slices.transition()
    .duration(800)
    .style('opacity', 1)
    .attrTween('d', function(d) {
        const i = d3.interpolate({ startAngle: 0, endAngle: 0 }, d);
        return function(t) { return arcGenerator(i(t) as any) as string; };
    });
    
  // Interactions
  slices
    .on('mouseover', function(_event, d) {
        d3.select(this)
          .transition()
          .duration(200)
          .attr('d', arcHover as any)
          .style('opacity', 0.8);
          
        // Add tooltip or update center text
        svg.select('.center-text-value')
          .text(formatCurrency(d.data.value));
        svg.select('.center-text-label')
          .text(d.data.label);
    })
    .on('mouseout', function(_event, _d) {
        d3.select(this)
          .transition()
          .duration(200)
          .attr('d', arcGenerator as any)
          .style('opacity', 1);
          
        // Reset center text
        svg.select('.center-text-value')
          .text(props.total ? formatCurrency(props.total) : '');
        svg.select('.center-text-label')
          .text(props.title || '');
    });
    
  // Center Text (Default)
  svg.append('text')
    .attr('class', 'center-text-value')
    .attr('text-anchor', 'middle')
    .attr('y', -5)
    .style('font-size', '16px')
    .style('font-weight', 'bold')
    .style('fill', 'currentColor')
    .text(props.total ? formatCurrency(props.total) : '');
    
  svg.append('text')
    .attr('class', 'center-text-label')
    .attr('text-anchor', 'middle')
    .attr('y', 20)
    .style('font-size', '12px')
    .style('fill', '#888')
    .text(props.title || '');
};

watch(() => props.data, () => {
  drawChart();
}, { deep: true });

onMounted(() => {
  // Add resize listener
  const resizeObserver = new ResizeObserver(() => drawChart());
  if (chartContainer.value) {
    resizeObserver.observe(chartContainer.value);
  }
  drawChart();
});
</script>

<template>
  <div class="w-full h-full min-h-[250px] relative flex flex-col items-center">
      <div v-if="!data.length" class="absolute inset-0 flex flex-col items-center justify-center text-gray-400">
          <p class="text-sm">No transaction data yet</p>
      </div>
      <div ref="chartContainer" class="w-full h-full flex-1"></div>
      
      <!-- Legend -->
      <div v-if="data.length" class="w-full max-w-xs mt-4 flex flex-wrap justify-center gap-x-4 gap-y-2">
          <div v-for="(item, i) in data" :key="item.label" class="flex items-center gap-2 text-xs">
              <span class="w-3 h-3 rounded-full" :style="{ backgroundColor: colorPalette[i % colorPalette.length] }"></span>
              <span class="text-gray-600 dark:text-gray-300">{{ item.label }}</span>
          </div>
      </div>
  </div>
</template>
