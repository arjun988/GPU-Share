package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"sync"
	"time"
)

// GPUMesh control plane — rendezvous + dashboard API (Phases 2/5/6).
// Never executes GPU workloads — metadata only.

type Announce struct {
	NodeID       string   `json:"node_id"`
	NodeName     string   `json:"node_name"`
	PublicKeyHex string   `json:"public_key_hex"`
	Addrs        []string `json:"addrs"`
	Sharing      bool     `json:"sharing"`
	GPUModel     *string  `json:"gpu_model"`
	VRAMMB       *uint64  `json:"vram_mb"`
	VRAMFreeMB   *uint64  `json:"vram_free_mb"`
	Utilization  *uint32  `json:"utilization"`
}

type Peer struct {
	NodeID      string   `json:"node_id"`
	NodeName    string   `json:"node_name"`
	Addrs       []string `json:"addrs"`
	GPUModel    *string  `json:"gpu_model"`
	VRAMMB      *uint64  `json:"vram_mb"`
	VRAMFreeMB  *uint64  `json:"vram_free_mb"`
	Utilization *uint32  `json:"utilization"`
	Sharing     bool     `json:"sharing"`
}

type GPUInfo struct {
	Index        uint32  `json:"index"`
	Name         string  `json:"name"`
	VRAMTotalMB  uint64  `json:"vram_total_mb"`
	VRAMUsedMB   uint64  `json:"vram_used_mb"`
	VRAMFreeMB   uint64  `json:"vram_free_mb"`
	Utilization  *uint32 `json:"utilization"`
	TemperatureC *uint32 `json:"temperature_c"`
	NodeID       string  `json:"node_id"`
	NodeName     string  `json:"node_name"`
}

type JobInfo struct {
	JobID     string  `json:"job_id"`
	Peer      *string `json:"peer"`
	State     string  `json:"state"`
	ExitCode  *int    `json:"exit_code"`
	Image     string  `json:"image"`
	CreatedAt string  `json:"created_at"`
	NodeID    string  `json:"node_id"`
}

type GroupInfo struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Members     int    `json:"members"`
	OwnerNodeID string `json:"owner_node_id"`
}

type SyncPayload struct {
	Node   Announce    `json:"node"`
	GPUs   []GPUInfo   `json:"gpus"`
	Peers  []Peer      `json:"peers"`
	Jobs   []JobInfo   `json:"jobs"`
	Groups []GroupInfo `json:"groups"`
}

type Overview struct {
	GPUsOnline     int    `json:"gpus_online"`
	GPUsAvailable  int    `json:"gpus_available"`
	RunningJobs    int    `json:"running_jobs"`
	TotalVRAMGB    uint64 `json:"total_vram_gb"`
	Peers          int    `json:"peers"`
	Groups         int    `json:"groups"`
	Nodes          int    `json:"nodes"`
	UpdatedAt      string `json:"updated_at"`
}

type store struct {
	mu      sync.RWMutex
	peers   map[string]peerEntry
	gpus    []GPUInfo
	jobs    []JobInfo
	groups  []GroupInfo
	nodes   map[string]Announce
}

type peerEntry struct {
	Peer
	UpdatedAt time.Time
}

func main() {
	s := &store{
		peers: make(map[string]peerEntry),
		nodes: make(map[string]Announce),
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
	})

	// CORS wrapper
	withCORS := func(h http.HandlerFunc) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			origin := os.Getenv("GPUMESH_CORS_ORIGIN")
			if origin == "" {
				origin = "*"
			}
			w.Header().Set("Access-Control-Allow-Origin", origin)
			w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
			w.Header().Set("Access-Control-Allow-Headers", "Content-Type")
			if r.Method == http.MethodOptions {
				w.WriteHeader(http.StatusNoContent)
				return
			}
			h(w, r)
		}
	}

	mux.HandleFunc("/v1/announce", withCORS(s.handleAnnounce))
	mux.HandleFunc("/v1/peers/", withCORS(s.handlePeer))
	mux.HandleFunc("/v1/peers", withCORS(s.handleListPeers))
	mux.HandleFunc("/v1/sync", withCORS(s.handleSync))
	mux.HandleFunc("/v1/overview", withCORS(s.handleOverview))
	mux.HandleFunc("/v1/gpus", withCORS(s.handleGPUs))
	mux.HandleFunc("/v1/jobs", withCORS(s.handleJobs))
	mux.HandleFunc("/v1/groups", withCORS(s.handleGroups))
	mux.HandleFunc("/v1/nodes", withCORS(s.handleNodes))
	mux.HandleFunc("/v1/network", withCORS(s.handleNetwork))
	mux.HandleFunc("/v1/usage", withCORS(s.handleUsage))
	mux.HandleFunc("/v1/settings", withCORS(s.handleSettings))

	addr := ":8080"
	if v := os.Getenv("GPUMESH_API_ADDR"); v != "" {
		addr = v
	}
	log.Printf("gpumesh control-plane listening on %s (dashboard API ready)", addr)
	log.Fatal(http.ListenAndServe(addr, mux))
}

func (s *store) handleAnnounce(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var ann Announce
	if err := json.NewDecoder(r.Body).Decode(&ann); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if ann.NodeID == "" {
		http.Error(w, "node_id required", http.StatusBadRequest)
		return
	}
	s.mu.Lock()
	s.nodes[ann.NodeID] = ann
	s.peers[ann.NodeID] = peerEntry{
		Peer: Peer{
			NodeID:      ann.NodeID,
			NodeName:    ann.NodeName,
			Addrs:       ann.Addrs,
			GPUModel:    ann.GPUModel,
			VRAMMB:      ann.VRAMMB,
			VRAMFreeMB:  ann.VRAMFreeMB,
			Utilization: ann.Utilization,
			Sharing:     ann.Sharing,
		},
		UpdatedAt: time.Now(),
	}
	s.mu.Unlock()
	w.WriteHeader(http.StatusNoContent)
}

func (s *store) handleSync(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var payload SyncPayload
	if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if payload.Node.NodeID != "" {
		s.nodes[payload.Node.NodeID] = payload.Node
		s.peers[payload.Node.NodeID] = peerEntry{
			Peer: Peer{
				NodeID:      payload.Node.NodeID,
				NodeName:    payload.Node.NodeName,
				Addrs:       payload.Node.Addrs,
				GPUModel:    payload.Node.GPUModel,
				VRAMMB:      payload.Node.VRAMMB,
				VRAMFreeMB:  payload.Node.VRAMFreeMB,
				Utilization: payload.Node.Utilization,
				Sharing:     payload.Node.Sharing,
			},
			UpdatedAt: time.Now(),
		}
		for i := range payload.GPUs {
			payload.GPUs[i].NodeID = payload.Node.NodeID
			payload.GPUs[i].NodeName = payload.Node.NodeName
		}
		// Replace GPUs for this node
		filtered := make([]GPUInfo, 0, len(s.gpus))
		for _, g := range s.gpus {
			if g.NodeID != payload.Node.NodeID {
				filtered = append(filtered, g)
			}
		}
		s.gpus = append(filtered, payload.GPUs...)

		filteredJobs := make([]JobInfo, 0, len(s.jobs))
		for _, j := range s.jobs {
			if j.NodeID != payload.Node.NodeID {
				filteredJobs = append(filteredJobs, j)
			}
		}
		for i := range payload.Jobs {
			payload.Jobs[i].NodeID = payload.Node.NodeID
		}
		s.jobs = append(filteredJobs, payload.Jobs...)
		s.groups = payload.Groups
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *store) handleListPeers(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]Peer, 0, len(s.peers))
	for _, e := range s.peers {
		if time.Since(e.UpdatedAt) > 30*time.Minute {
			continue
		}
		out = append(out, e.Peer)
	}
	writeJSON(w, out)
}

func (s *store) handlePeer(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	id := r.URL.Path[len("/v1/peers/"):]
	if id == "" {
		http.NotFound(w, r)
		return
	}
	s.mu.RLock()
	e, ok := s.peers[id]
	s.mu.RUnlock()
	if !ok {
		http.NotFound(w, r)
		return
	}
	writeJSON(w, e.Peer)
}

func (s *store) handleOverview(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	var totalVRAM uint64
	available := 0
	for _, g := range s.gpus {
		totalVRAM += g.VRAMTotalMB
		if g.Utilization == nil || *g.Utilization < 50 {
			available++
		}
	}
	running := 0
	for _, j := range s.jobs {
		if j.State == "RUNNING" || j.State == "STARTING" {
			running++
		}
	}
	writeJSON(w, Overview{
		GPUsOnline:    len(s.gpus),
		GPUsAvailable: available,
		RunningJobs:   running,
		TotalVRAMGB:   totalVRAM / 1024,
		Peers:         len(s.peers),
		Groups:        len(s.groups),
		Nodes:         len(s.nodes),
		UpdatedAt:     time.Now().UTC().Format(time.RFC3339),
	})
}

func (s *store) handleGPUs(w http.ResponseWriter, _ *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	writeJSON(w, s.gpus)
}

func (s *store) handleJobs(w http.ResponseWriter, _ *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	writeJSON(w, s.jobs)
}

func (s *store) handleGroups(w http.ResponseWriter, _ *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	writeJSON(w, s.groups)
}

func (s *store) handleNodes(w http.ResponseWriter, _ *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]Announce, 0, len(s.nodes))
	for _, n := range s.nodes {
		out = append(out, n)
	}
	writeJSON(w, out)
}

func (s *store) handleNetwork(w http.ResponseWriter, _ *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	writeJSON(w, map[string]any{
		"nodes":       len(s.nodes),
		"peers":       len(s.peers),
		"groups":      len(s.groups),
		"control":     "rendezvous+metadata",
		"workloads":   false,
		"description": "Control plane never runs GPU workloads",
	})
}

func (s *store) handleUsage(w http.ResponseWriter, _ *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	succeeded, failed := 0, 0
	for _, j := range s.jobs {
		switch j.State {
		case "SUCCEEDED":
			succeeded++
		case "FAILED":
			failed++
		}
	}
	writeJSON(w, map[string]any{
		"jobs_total":     len(s.jobs),
		"jobs_succeeded": succeeded,
		"jobs_failed":    failed,
		"nodes":          len(s.nodes),
	})
}

func (s *store) handleSettings(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, map[string]any{
		"product":     "GPUMesh Cloud",
		"phase":       "6",
		"api_version": "v1",
		"security":    "Ed25519 peer identity; workloads stay on provider nodes",
	})
}

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}
