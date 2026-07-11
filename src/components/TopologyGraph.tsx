import { useCallback, useEffect, useState, type FC, type MouseEvent } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  Node,
  Edge,
  type NodeMouseHandler,
} from '@xyflow/react';
import { ApiService, GraphEdge, GraphNode } from '../services/api';
import { NodeIntelDrawer } from './NodeIntelDrawer';
import '@xyflow/react/dist/style.css';

interface TopologyGraphProps {
  onNavigateToTriage?: () => void;
}

export const TopologyGraph: FC<TopologyGraphProps> = ({ onNavigateToTriage }) => {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [graphNodes, setGraphNodes] = useState<GraphNode[]>([]);
  const [graphEdges, setGraphEdges] = useState<GraphEdge[]>([]);
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const buildFlowNodes = useCallback(
    (rawNodes: GraphNode[], selectedId: string | null): Node[] =>
      rawNodes.map((node, index) => {
        let nodeBg = 'bg-slate-900 border-slate-700 text-slate-300';
        if (node.entity_type === 'vector') {
          nodeBg = 'bg-rose-950/80 border-rose-800 text-rose-300';
        } else if (node.entity_type === 'indicator') {
          nodeBg = 'bg-amber-950/80 border-amber-800 text-amber-300';
        } else if (node.entity_type === 'legal') {
          nodeBg = 'bg-indigo-950/80 border-indigo-800 text-indigo-300';
        }

        const selected = node.id === selectedId;

        return {
          id: node.id,
          type: 'default',
          selected,
          data: {
            graphNode: node,
            label: (
              <div className="p-1 font-mono text-left">
                <div className="text-[9px] uppercase tracking-wider opacity-60">
                  [{node.entity_type}]
                </div>
                <div className="text-xs font-bold truncate max-w-[160px]">{node.title}</div>
              </div>
            ),
          },
          position: { x: (index % 3) * 250, y: Math.floor(index / 3) * 150 },
          className: `${nodeBg} border rounded-md shadow-md w-48 font-mono shadow-black/40 ${
            selected ? 'ring-2 ring-slate-200 ring-offset-1 ring-offset-slate-950' : ''
          }`,
        };
      }),
    [],
  );

  useEffect(() => {
    async function loadGraphData() {
      try {
        const graphData = await ApiService.getKnowledgeGraph();
        setGraphNodes(graphData.nodes);
        setGraphEdges(graphData.edges);
        setNodes(buildFlowNodes(graphData.nodes, null));

        const mappedEdges: Edge[] = graphData.edges.map((edge, index) => ({
          id: `e-${index}`,
          source: edge.source_node_id,
          target: edge.target_node_id,
          label: edge.relationship_type.toLowerCase(),
          animated:
            edge.relationship_type === 'EXPLOITS' || edge.relationship_type === 'TRIGGERS',
          style: { stroke: '#475569', strokeWidth: 1.5 },
          labelStyle: {
            fill: '#94a3b8',
            fontSize: 9,
            fontFamily: 'monospace',
            fillOpacity: 0.7,
          },
        }));

        setEdges(mappedEdges);
      } catch (err) {
        console.error('Failed to construct system visualization topology:', err);
      } finally {
        setIsLoading(false);
      }
    }

    loadGraphData();
  }, [buildFlowNodes]);

  useEffect(() => {
    setNodes(buildFlowNodes(graphNodes, selectedNode?.id ?? null));
  }, [selectedNode, graphNodes, buildFlowNodes]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setSelectedNode(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const onNodeClick: NodeMouseHandler = useCallback(
    (_event: MouseEvent, node: Node) => {
      const graphNode = graphNodes.find((n) => n.id === node.id);
      if (graphNode) setSelectedNode(graphNode);
    },
    [graphNodes],
  );

  const onPaneClick = useCallback(() => setSelectedNode(null), []);

  if (isLoading) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-slate-950 text-slate-500 font-mono text-xs">
        Compiling topology link structures from local graph indices...
      </div>
    );
  }

  return (
    <div className="relative flex h-full w-full flex-col bg-slate-950">
      <div className="pointer-events-none absolute left-4 top-4 z-10 max-w-sm rounded-md border border-slate-800 bg-slate-900/90 p-4 font-mono shadow-xl backdrop-blur">
        <div className="mb-1 text-xs font-bold uppercase tracking-wider text-rose-400">
          // RISK TOPOLOGY GRAPH VIEW
        </div>
        <p className="text-[11px] leading-relaxed text-slate-400">
          Click any node to open intelligence analytics. Animated edges signify operational live
          vectors.
        </p>
        <div className="mt-3 flex flex-wrap gap-2 text-[9px] font-bold">
          <span className="rounded border border-rose-800 bg-rose-950 px-1.5 py-0.5 text-rose-400">
            ATTACK_VECTOR
          </span>
          <span className="rounded border border-amber-800 bg-amber-950 px-1.5 py-0.5 text-amber-400">
            INDICATOR_SIGNAL
          </span>
          <span className="rounded border border-indigo-800 bg-indigo-950 px-1.5 py-0.5 text-indigo-400">
            COMPLIANCE_LEGAL
          </span>
        </div>
      </div>

      <div className="h-full w-full flex-1">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          fitView
          minZoom={0.2}
          maxZoom={1.5}
        >
          <Background color="#334155" gap={20} size={1} />
          <Controls className="rounded border border-slate-800 bg-slate-900 fill-current text-slate-400 [&_button:hover]:bg-slate-800 [&_button]:border-b [&_button]:border-slate-800" />
        </ReactFlow>
      </div>

      {selectedNode && (
        <NodeIntelDrawer
          node={selectedNode}
          edges={graphEdges}
          allNodes={graphNodes}
          onClose={() => setSelectedNode(null)}
          onNavigateToTriage={onNavigateToTriage}
        />
      )}
    </div>
  );
};
