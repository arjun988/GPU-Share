package main

import (
	"encoding/json"
	"log"
	"net/http"
	"sync"
	"time"
)

// Minimal rendezvous / signaling control plane (Phase 2+).
// Never executes GPU workloads — metadata and discovery only.

type Announce struct {
	NodeID        string   `json:"node_id"`
	NodeName      string   `json:"node_name"`
	PublicKeyHex  string   `json:"public_key_hex"`
	Addrs         []string `json:"addrs"`
	Sharing       bool     `json:"sharing"`
	GPUModel      *string  `json:"gpu_model"`
	VRAMMB        *uint64  `json:"vram_mb"`
}

type Peer struct {
	NodeID   string   `json:"node_id"`
	NodeName string   `json:"node_name"`
	Addrs    []string `json:"addrs"`
	GPUModel *string  `json:"gpu_model"`
	VRAMMB   *uint64  `json:"vram_mb"`
	Sharing  bool     `json:"sharing"`
}

type store struct {
	mu    sync.RWMutex
	peers map[string]peerEntry
}

type peerEntry struct {
	Peer
	UpdatedAt time.Time
}

func main() {
	s := &store{peers: make(map[string]peerEntry)}
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
	})
	mux.HandleFunc("/v1/announce", s.handleAnnounce)
	mux.HandleFunc("/v1/peers/", s.handlePeer)
	mux.HandleFunc("/v1/peers", s.handleList)

	addr := ":8080"
	log.Printf("gpumesh control-plane (rendezvous) listening on %s", addr)
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
	s.peers[ann.NodeID] = peerEntry{
		Peer: Peer{
			NodeID:   ann.NodeID,
			NodeName: ann.NodeName,
			Addrs:    ann.Addrs,
			GPUModel: ann.GPUModel,
			VRAMMB:   ann.VRAMMB,
			Sharing:  ann.Sharing,
		},
		UpdatedAt: time.Now(),
	}
	s.mu.Unlock()
	w.WriteHeader(http.StatusNoContent)
}

func (s *store) handleList(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]Peer, 0, len(s.peers))
	for _, e := range s.peers {
		if time.Since(e.UpdatedAt) > 10*time.Minute {
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

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}
