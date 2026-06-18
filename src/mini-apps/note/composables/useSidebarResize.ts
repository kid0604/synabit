import { ref } from 'vue';

export function useSidebarResize() {
  const wNoteSidebar = ref(300);
  const showNoteSidebar = ref(window.innerWidth >= 768);
  const wRightSidebar = ref(288);
  const showRightSidebar = ref(window.innerWidth >= 768);

  const isDraggingNoteSidebar = ref(false);
  const startDragNoteSidebar = () => { isDraggingNoteSidebar.value = true; };
  const isDraggingRightSidebar = ref(false);
  const startDragRightSidebar = () => { isDraggingRightSidebar.value = true; };

  const onMouseMove = (e: MouseEvent) => {
    if (isDraggingNoteSidebar.value) {
      wNoteSidebar.value = Math.max(220, Math.min(e.clientX - 64, 600));
    } else if (isDraggingRightSidebar.value) {
      wRightSidebar.value = Math.max(200, Math.min(window.innerWidth - e.clientX, 600));
    }
  };

  const onMouseUp = () => {
    isDraggingNoteSidebar.value = false;
    isDraggingRightSidebar.value = false;
  };

  return {
    wNoteSidebar,
    showNoteSidebar,
    wRightSidebar,
    showRightSidebar,
    isDraggingNoteSidebar,
    isDraggingRightSidebar,
    startDragNoteSidebar,
    startDragRightSidebar,
    onMouseMove,
    onMouseUp,
  };
}
