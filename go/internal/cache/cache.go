// Package cache provides an in-memory TTL cache with ETag support.
package cache

import (
	"crypto/md5"
	"fmt"
	"sync"
	"time"
)

// TTL constants matching the Python implementation.
const (
	TTLEntityInfo    = 24 * time.Hour // Profiles, bootstrap — rarely change
	TTLCurrentSeason = 1 * time.Hour  // Current season stats
	TTLHistorical    = 24 * time.Hour // Historical season stats
	TTLNews          = 10 * time.Minute
)

type entry struct {
	data      []byte
	etag      string
	expiresAt time.Time
}

// Cache is a thread-safe in-memory TTL cache.
type Cache struct {
	mu      sync.RWMutex
	entries map[string]entry
	enabled bool
}

// New creates a new cache. Pass enabled=false to create a no-op cache.
func New(enabled bool) *Cache {
	c := &Cache{
		entries: make(map[string]entry),
		enabled: enabled,
	}
	if enabled {
		go c.evictLoop()
	}
	return c
}

// Get retrieves a cached value. Returns data, etag, and whether the entry was found.
func (c *Cache) Get(key string) (data []byte, etag string, ok bool) {
	if !c.enabled {
		return nil, "", false
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	e, exists := c.entries[key]
	if !exists || time.Now().After(e.expiresAt) {
		return nil, "", false
	}
	return e.data, e.etag, true
}

// Set stores a value with a TTL.
func (c *Cache) Set(key string, data []byte, ttl time.Duration) string {
	if !c.enabled {
		return ComputeETag(data)
	}
	etag := ComputeETag(data)
	c.mu.Lock()
	defer c.mu.Unlock()
	c.entries[key] = entry{
		data:      data,
		etag:      etag,
		expiresAt: time.Now().Add(ttl),
	}
	return etag
}

// Stats returns cache statistics.
func (c *Cache) Stats() map[string]interface{} {
	c.mu.RLock()
	defer c.mu.RUnlock()

	active := 0
	now := time.Now()
	for _, e := range c.entries {
		if now.Before(e.expiresAt) {
			active++
		}
	}
	return map[string]interface{}{
		"enabled":      c.enabled,
		"total_keys":   len(c.entries),
		"active_keys":  active,
		"expired_keys": len(c.entries) - active,
	}
}

// evictLoop periodically removes expired entries.
func (c *Cache) evictLoop() {
	ticker := time.NewTicker(5 * time.Minute)
	defer ticker.Stop()
	for range ticker.C {
		c.evict()
	}
}

func (c *Cache) evict() {
	c.mu.Lock()
	defer c.mu.Unlock()
	now := time.Now()
	for key, e := range c.entries {
		if now.After(e.expiresAt) {
			delete(c.entries, key)
		}
	}
}

// ComputeETag generates a weak ETag from response data using MD5.
func ComputeETag(data []byte) string {
	hash := md5.Sum(data)
	return fmt.Sprintf(`W/"%x"`, hash[:8])
}

// CheckETagMatch checks if If-None-Match header matches the current ETag.
func CheckETagMatch(ifNoneMatch, etag string) bool {
	if ifNoneMatch == "" {
		return false
	}
	if ifNoneMatch == "*" {
		return true
	}
	// Simple comparison — handles the common single-etag case
	return ifNoneMatch == etag
}
