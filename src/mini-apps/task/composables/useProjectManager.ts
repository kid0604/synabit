import { ref, computed, watch, type Ref, type ComputedRef } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { type TaskMetadata, formatNumber } from '../types';
import type { Transaction, FinanceAccount, Category } from '../../finance/types';
import { DEFAULT_INCOME_CATEGORIES, DEFAULT_EXPENSE_CATEGORIES, DEFAULT_ACCOUNTS } from '../../finance/types';
import { toCategories } from '../../finance/categories';
import { logger } from '../../../utils/logger';
import { i18n } from '../../../i18n';

const t = i18n.global.t;

export function useProjectManager(
  activeCategory: Ref<string>,
  activeCategoryTasks: ComputedRef<TaskMetadata[]>,
  projects: Ref<any[]>,
  ns: any,
  vaultPath: Ref<string>,
  emit: (event: string, ...args: any[]) => void,
  loadTasks: () => Promise<void>,
) {
  const activeProjectTab = ref<'overview' | 'tasks' | 'resources' | 'notes'>('overview');
  const newProjectDraft = ref<any>(null);
  const showProjectEditModal = ref(false);

  const showEmbedPicker = ref(false);
  const allNotesForPicker = ref<any[]>([]);
  const isLinkingResource = ref(false);
  const showAddResourceMenu = ref(false);
  const showEmptyAddMenu = ref(false);

  const showTxModal = ref(false);
  // `toCategories`, not the bare constants: DEFAULT_*_CATEGORIES are lists of
  // names, and the transaction form needs `{ id, name }`. Seeded with strings,
  // a project in a vault with no Finance config opened a category dropdown of
  // blank rows. See `finance/categories.ts`.
  const incomeCategories = ref<Category[]>(toCategories(DEFAULT_INCOME_CATEGORIES));
  const expenseCategories = ref<Category[]>(toCategories(DEFAULT_EXPENSE_CATEGORIES));
  const accounts = ref<FinanceAccount[]>([...DEFAULT_ACCOUNTS]);

  const activeProject = computed(() => {
    if (activeCategory.value.startsWith('project:')) {
      const id = activeCategory.value.substring(8);
      return projects.value.find(p => p.id === id);
    }
    return null;
  });

  const projectProgress = computed(() => {
    if (!activeCategoryTasks.value || activeCategoryTasks.value.length === 0) return 0;
    const total = activeCategoryTasks.value.length;
    const done = activeCategoryTasks.value.filter(t => t.status === 'done').length;
    return Math.round((done / total) * 100);
  });

  const projectCurrency = computed(() => {
    if (!activeProject.value || !activeProject.value.custom_fields) return 'VND';
    const keys = Object.keys(activeProject.value.custom_fields);
    const currKey = keys.find(k => k.toLowerCase() === 'currency');
    return currKey ? activeProject.value.custom_fields[currKey] || 'VND' : 'VND';
  });

  const projectBudget = computed(() => {
    if (!activeProject.value || !activeProject.value.custom_fields) return null;
    const keys = Object.keys(activeProject.value.custom_fields);
    const budgetKey = keys.find(k => k.toLowerCase() === 'budget');
    if (budgetKey && activeProject.value.custom_fields[budgetKey]) {
      return formatNumber(activeProject.value.custom_fields[budgetKey]) + ' ' + projectCurrency.value;
    }
    return null;
  });

  const calculatedProjectSpent = ref(0);

  const projectSpent = computed(() => {
    return (formatNumber(calculatedProjectSpent.value) || '0') + ' ' + projectCurrency.value;
  });

  const displayCustomFields = computed(() => {
    if (!activeProject.value || !activeProject.value.custom_fields) return [];
    const exclude = ['title', 'type', 'created_at', 'updated_at', 'status', 'start_date', 'due_date', 'color', 'tags', 'project_id', 'completed_at', 'order', 'budget', 'spent', 'wip_limit', 'currency', 'id', 'path', 'content'];
    
    const fields: {key: string, val: any}[] = [];
    for (const [key, val] of Object.entries(activeProject.value.custom_fields)) {
      if (!exclude.includes(key.toLowerCase())) {
        fields.push({ key, val });
      }
    }
    return fields;
  });

  const linkedResources = ref<any[]>([]);
  let fetchNotesTimeout: any = null;

  watch(activeProject, (proj, oldProj) => {
    if (proj && proj.id !== oldProj?.id) {
      activeProjectTab.value = 'overview';
    }
    clearTimeout(fetchNotesTimeout);
    if (proj) {
      fetchNotesTimeout = setTimeout(async () => {
        await loadProjectResources();
        
        // Fetch finance transactions for dynamic spent calculation
        recalculateProjectSpent(proj);
      }, 100);
    } else {
      linkedResources.value = [];
      calculatedProjectSpent.value = 0;
    }
  }, { immediate: true });

  const loadProjectResources = async () => {
    if (!activeProject.value) return;
    try {
      const edges = await ns.getLinkedNodes(activeProject.value.title, activeProject.value.id);
      // Boards arrive already typed as 'whiteboard'. This used to receive some
      // of them typed 'json' and relabel them here, because boards were indexed
      // by two different code paths that did not agree; there is one path now.
      linkedResources.value = edges.filter((n: any) =>
        ['note', 'whiteboard', 'file'].includes(n.node_type)
      );
    } catch(e) {
      console.error('Failed to get linked resources', e);
    }
  };

  const recalculateProjectSpent = async (proj: any) => {
    try {
      const financeNodes = await ns.getNodes('finance_month');
      let totalSpent = 0;
      for (const node of financeNodes) {
        if (node.properties?.transactions) {
          for (const tx of node.properties.transactions) {
            if (tx.projectId === proj.id && tx.type === 'expense') {
              totalSpent += tx.amount;
            }
          }
        }
      }
      calculatedProjectSpent.value = totalSpent;
    } catch (e) {
      console.error('Failed to get finance data for project spent', e);
    }
  };

  const handleCreateProjectClick = () => {
    newProjectDraft.value = {
      title: '',
      content: '',
      due_date: '',
      start_date: '',
      status: 'active',
      isNew: true
    };
    showProjectEditModal.value = true;
  };

  const handleProjectSave = async (updatedProject: any) => {
    try {
      if (newProjectDraft.value) {
        // Create new project
        if (!updatedProject.title.trim()) updatedProject.title = t('task.untitled_project');
        const relPath = `Projects/${crypto.randomUUID()}.md`;
        await ns.writeNode({
          relPath: relPath,
          nodeType: 'project',
          title: updatedProject.title,
          properties: {
            status: updatedProject.status,
            start_date: updatedProject.start_date,
            due_date: updatedProject.due_date,
            tags: updatedProject.tags,
            color: '',
            ...(updatedProject.custom_fields || {})
          },
          content: updatedProject.content,
          eventType: 'created'
        });
        
        showProjectEditModal.value = false;
        newProjectDraft.value = null;
        await loadTasks();
        
        // Open the newly created project
        const newProj = projects.value.find(p => p.path === relPath);
        if (newProj) {
          activeCategory.value = 'project:' + newProj.id;
        }
      } else if (activeProject.value) {
        // Update existing project
        await ns.writeNode({
          relPath: activeProject.value.path,
          nodeType: 'project',
          title: updatedProject.title,
          properties: {
            status: updatedProject.status,
            start_date: updatedProject.start_date,
            due_date: updatedProject.due_date,
            tags: updatedProject.tags,
            color: activeProject.value.color || '',
            ...(updatedProject.custom_fields || {})
          },
          content: updatedProject.content
        });
        showProjectEditModal.value = false;
        await loadTasks();
      }
    } catch (e) {
      logger.error("Failed to save project", e);
    }
  };

  const deleteProject = async () => {
    if (!activeProject.value) return;
    let isConfirmed = false;
    try {
      isConfirmed = await ask(t('task.delete_project_body'), {
        title: t('task.delete_project_title'),
        kind: 'warning',
        okLabel: t('task.delete_confirm'),
        cancelLabel: t('task.delete_cancel')
      });
    } catch (e) {
      logger.warn("Tauri confirm failed, falling back to window.confirm", e);
      isConfirmed = window.confirm(t('task.delete_project_title'));
    }
    
    if (!isConfirmed) return;
    
    try {
      // The trash, not an unlink — see `deleteTask`.
      await ns.trashNode({ relPath: activeProject.value.path });
      showProjectEditModal.value = false;
      activeCategory.value = 'all';
      await loadTasks();
    } catch (e) {
      logger.error("Failed to delete project", e);
    }
  };

  const openLinkResourcePicker = async () => {
    try {
      isLinkingResource.value = true;
      const resultNotes = await ns.getNodes('note');
      const resultWhiteboards = await invoke<any[]>('scan_whiteboards', { vaultPath: vaultPath.value });
      resultWhiteboards.forEach(w => w.node_type = 'whiteboard');
      const resultFiles = await ns.getNodes('file');
      const allResources = [...resultNotes, ...resultWhiteboards, ...resultFiles];
      
      const linkedResourceIds = new Set(linkedResources.value.map(n => n.id));
      allNotesForPicker.value = allResources.filter(n => !linkedResourceIds.has(n.id));
      showEmbedPicker.value = true;
    } catch(e) {
      logger.error("Failed to load resources for picker", e);
    } finally {
      isLinkingResource.value = false;
    }
  };

  const createNewResourceNote = async () => {
    if (!vaultPath.value || !activeProject.value) return;
    try {
      isLinkingResource.value = true;
      // Create new node file
      const newPath = await ns.createNode({ 
        directory: 'Notes', 
        nodeType: 'note' 
      });
      
      // Read it back to get default properties
      const node = await ns.getNode(newPath);
      if (node) {
        const propsObj = node.properties || {};
        const projectsArray = Array.isArray(propsObj.linked_projects) ? propsObj.linked_projects : [];
        const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
        
        if (!projectsArray.includes(projectLink)) {
          projectsArray.push(projectLink);
          propsObj.linked_projects = projectsArray;
          
          await ns.writeNode({
            relPath: node.id,
            title: node.title,
            nodeType: 'note',
            properties: propsObj,
            content: node.content
          });
        }
      }
      
      // Reload linked resources
      await loadProjectResources();
      emit('open-node', newPath, 'note'); // Optionally open it immediately
    } catch(e) {
      logger.error("Failed to create resource note", e);
    } finally {
      isLinkingResource.value = false;
    }
  };

  const createNewResourceWhiteboard = async () => {
    if (!vaultPath.value || !activeProject.value) return;
    try {
      isLinkingResource.value = true;
      
      const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
      const title = 'New Whiteboard';
      const data = {
        title: title,
        type: 'whiteboard',
        metadata: {
          linked_projects: [projectLink],
          // Sync settles two copies of a board by this stamp; a board that
          // reaches another device without one cannot win a comparison.
          updated_at: new Date().toISOString(),
        },
        tags: [],
        created_at: new Date().toISOString(),
        viewport: { x: 0, y: 0, zoom: 1 },
        nodes: [],
        edges: [],
      };
      const content = JSON.stringify(data, null, 2);
      
      const meta = await invoke<any>('create_whiteboard', {
        vaultPath: vaultPath.value,
        title: title,
        tags: [],
        content: content
      });
      
      // Scan the new file so that its graph edges (links to project) are indexed
      await ns.scanSpecificNodes([meta.path]);
      
      // Reload linked resources
      await loadProjectResources();
      emit('open-node', meta.path, 'whiteboard'); // Optionally open it immediately
    } catch(e) {
      logger.error("Failed to create resource whiteboard", e);
    } finally {
      isLinkingResource.value = false;
    }
  };

  const unlinkResource = async (node: any) => {
    if (!activeProject.value) return;
    
    const confirmed = await ask(`"${node.title || 'This resource'}" will no longer be linked to this project.`, {
      title: 'Unlink resource?',
      kind: 'warning',
      okLabel: 'Unlink',
      cancelLabel: 'Cancel'
    });
    if (!confirmed) return;

    try {
      const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
      
      if (node.node_type === 'whiteboard' && node.id.endsWith('.json')) {
        const rawContent = await invoke<string>('read_whiteboard', {
          vaultPath: vaultPath.value,
          path: node.id
        });
        const data = JSON.parse(rawContent);
        if (data.metadata?.linked_projects && Array.isArray(data.metadata.linked_projects)) {
          data.metadata.linked_projects = data.metadata.linked_projects.filter((l: string) => l !== projectLink);
          
          await invoke('update_whiteboard', {
            vaultPath: vaultPath.value,
            path: node.id,
            title: data.title,
            tags: data.tags || [],
            content: JSON.stringify(data, null, 2)
          });
          
          await ns.scanSpecificNodes([node.id]);
        }
      } else if (node.node_type === 'file') {
        const fetchedNode = await ns.getNode(node.id);
        if (fetchedNode) {
          const propsObj = fetchedNode.properties || {};
          if (Array.isArray(propsObj.linked_projects)) {
            propsObj.linked_projects = propsObj.linked_projects.filter((l: string) => l !== projectLink);
            await ns.updateFileNodeProperties(fetchedNode.id, propsObj);
          }
        }
      } else {
        // For notes, markdown-based nodes, and corrupted whiteboard .md files
        const fetchedNode = await ns.getNode(node.id);
        if (fetchedNode) {
          const propsObj = fetchedNode.properties || {};
          if (Array.isArray(propsObj.linked_projects)) {
            propsObj.linked_projects = propsObj.linked_projects.filter((l: string) => l !== projectLink);
            
            await ns.writeNode({
              relPath: fetchedNode.id,
              title: fetchedNode.title,
              nodeType: fetchedNode.node_type,
              properties: propsObj,
              content: fetchedNode.content
            });
          }
        }
      }
      
      await loadProjectResources();
    } catch (e) {
      logger.error('Failed to unlink resource', e);
    }
  };

  const handleEmbedResource = async (node: any) => {
    showEmbedPicker.value = false;
    if (!activeProject.value) return;
    try {
      isLinkingResource.value = true;
      const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
      
      if (node.node_type === 'whiteboard' && node.id.endsWith('.json')) {
        const rawContent = await invoke<string>('read_whiteboard', {
          vaultPath: vaultPath.value,
          path: node.id
        });
        const data = JSON.parse(rawContent);
        if (!data.metadata) data.metadata = {};
        
        const projectsArray = Array.isArray(data.metadata.linked_projects) ? data.metadata.linked_projects : [];
        if (!projectsArray.includes(projectLink)) {
          projectsArray.push(projectLink);
          data.metadata.linked_projects = projectsArray;
          
          await invoke('update_whiteboard', {
            vaultPath: vaultPath.value,
            path: node.id,
            title: data.title,
            tags: data.tags || [],
            content: JSON.stringify(data, null, 2)
          });
          
          await ns.scanSpecificNodes([node.id]);
        }
      } else if (node.node_type === 'file') {
        const fullNode = await ns.getNode(node.id);
        if (fullNode) {
          const propsObj = fullNode.properties || {};
          const projectsArray = Array.isArray(propsObj.linked_projects) ? propsObj.linked_projects : [];
          
          if (!projectsArray.includes(projectLink)) {
            projectsArray.push(projectLink);
            propsObj.linked_projects = projectsArray;
            
            await ns.updateFileNodeProperties(fullNode.id, propsObj);
          }
        }
      } else {
        // Since we already have the node from the modal, we could use it directly
        // but we still call get_node to get fresh properties and content
        const fullNode = await ns.getNode(node.id);
        if (fullNode) {
          const propsObj = fullNode.properties || {};
          const projectsArray = Array.isArray(propsObj.linked_projects) ? propsObj.linked_projects : [];
          
          if (!projectsArray.includes(projectLink)) {
            projectsArray.push(projectLink);
            propsObj.linked_projects = projectsArray;
            
            await ns.writeNode({
              relPath: node.id,
              title: fullNode.title,
              nodeType: fullNode.node_type || 'note',
              properties: propsObj,
              content: fullNode.content
            });
          }
        }
      }
      await loadProjectResources();
    } catch (e) {
      logger.error("Failed to link resource", e);
    } finally {
      isLinkingResource.value = false;
    }
  };

  const loadFinanceConfig = async () => {
    try {
      const configs: any[] = await ns.getNodes('finance_config');
      if (configs.length > 0) {
        const configNode = configs[0];
        if (configNode.properties) {
          if (configNode.properties.incomeCategories) {
            incomeCategories.value = toCategories(configNode.properties.incomeCategories);
          }
          if (configNode.properties.expenseCategories) {
            expenseCategories.value = toCategories(configNode.properties.expenseCategories);
          }
          if (configNode.properties.accounts) {
            accounts.value = configNode.properties.accounts;
          }
        }
      }
    } catch (e) {
      logger.error('Failed to load finance config in TaskApp', e);
    }
  };

  const saveFinanceTransaction = async (tx: Transaction) => {
    const d = new Date(tx.date);
    const mm = (d.getMonth() + 1).toString().padStart(2, '0');
    const yyyy = d.getFullYear();
    const expectedId = `Finance/${yyyy}-${mm}.json`;
    
    try {
      let nodeProps: any = { transactions: [] };
      try {
        const existingNodes = await ns.getNodes('finance_month');
        const targetNode = existingNodes.find((n: any) => n.id === expectedId);
        if (targetNode && targetNode.properties) {
          nodeProps = targetNode.properties;
        }
      } catch(e) {}
      
      if (!nodeProps.transactions) nodeProps.transactions = [];
      
      const existingIdx = nodeProps.transactions.findIndex((t: Transaction) => t.id === tx.id);
      if (existingIdx >= 0) {
        nodeProps.transactions[existingIdx] = tx;
      } else {
        nodeProps.transactions.push(tx);
      }
      
      await ns.writeNode({
        relPath: expectedId,
        title: `Tháng ${mm}/${yyyy}`,
        nodeType: 'finance_month',
        properties: nodeProps,
        content: '',
        silent: true
      });
      
      showTxModal.value = false;
      if (activeProject.value) {
        recalculateProjectSpent(activeProject.value);
      }
    } catch (e) {
      logger.error('Failed to save finance transaction from Task App', e);
    }
  };

  return {
    activeProject, activeProjectTab,
    projectProgress, projectBudget, projectSpent, projectCurrency, displayCustomFields,
    calculatedProjectSpent,
    linkedResources, loadProjectResources,
    showProjectEditModal, newProjectDraft,
    handleCreateProjectClick, handleProjectSave, deleteProject,
    showEmbedPicker, allNotesForPicker, isLinkingResource, showAddResourceMenu, showEmptyAddMenu,
    openLinkResourcePicker, createNewResourceNote, createNewResourceWhiteboard,
    unlinkResource, handleEmbedResource,
    showTxModal, incomeCategories, expenseCategories, accounts,
    loadFinanceConfig, saveFinanceTransaction,
  };
}
