package opencodeproxy

import (
	"bufio"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestProxyForwardsRequestAndStripsMountPath(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPatch {
			t.Fatalf("method = %s, want %s", r.Method, http.MethodPatch)
		}
		if r.URL.Path != "/session/abc" {
			t.Fatalf("path = %s, want /session/abc", r.URL.Path)
		}
		if r.URL.RawQuery != "q=search&limit=1" {
			t.Fatalf("query = %s, want q=search&limit=1", r.URL.RawQuery)
		}
		if r.Header.Get("X-Test-Header") != "kept" {
			t.Fatalf("X-Test-Header = %q, want kept", r.Header.Get("X-Test-Header"))
		}
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Fatalf("ReadAll() error = %v", err)
		}
		if string(body) != `{"ok":true}` {
			t.Fatalf("body = %s, want request body", body)
		}
		w.Header().Set("X-Upstream", "opencode")
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte("created"))
	}))
	defer upstream.Close()

	proxy := httptest.NewServer(NewHandler(mustParseTestURL(t, upstream.URL), MountPath))
	defer proxy.Close()

	req, err := http.NewRequest(http.MethodPatch, proxy.URL+MountPath+"/session/abc?q=search&limit=1", strings.NewReader(`{"ok":true}`))
	if err != nil {
		t.Fatalf("NewRequest() error = %v", err)
	}
	req.Header.Set("X-Test-Header", "kept")

	resp, err := proxy.Client().Do(req)
	if err != nil {
		t.Fatalf("Do() error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusCreated {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusCreated)
	}
	if resp.Header.Get("X-Upstream") != "opencode" {
		t.Fatalf("X-Upstream = %q, want opencode", resp.Header.Get("X-Upstream"))
	}
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("ReadAll() error = %v", err)
	}
	if string(body) != "created" {
		t.Fatalf("body = %s, want created", body)
	}
}

func TestProxyReturnsGatewayErrorWhenUnavailable(t *testing.T) {
	proxy := httptest.NewServer(NewHandler(mustParseTestURL(t, "http://127.0.0.1:1"), MountPath))
	defer proxy.Close()

	resp, err := proxy.Client().Get(proxy.URL + MountPath + "/session")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusBadGateway {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusBadGateway)
	}
	if got := resp.Header.Get("Content-Type"); !strings.Contains(got, "application/json") {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}
}

func TestProxyPreservesSSEStreaming(t *testing.T) {
	firstEventWritten := make(chan struct{})
	releaseUpstream := make(chan struct{})

	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		w.Write([]byte("data: first\n\n"))
		w.(http.Flusher).Flush()
		close(firstEventWritten)
		<-releaseUpstream
	}))
	defer upstream.Close()
	defer close(releaseUpstream)

	proxy := httptest.NewServer(NewHandler(mustParseTestURL(t, upstream.URL), MountPath))
	defer proxy.Close()

	client := proxy.Client()
	client.Timeout = 2 * time.Second
	resp, err := client.Get(proxy.URL + MountPath + "/events")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	defer resp.Body.Close()

	select {
	case <-firstEventWritten:
	case <-time.After(time.Second):
		t.Fatal("upstream did not write first event")
	}

	line, err := bufio.NewReader(resp.Body).ReadString('\n')
	if err != nil {
		t.Fatalf("ReadString() error = %v", err)
	}
	if line != "data: first\n" {
		t.Fatalf("first SSE line = %q, want data: first", line)
	}
}

func TestHostProxyKeepsRootMountedPath(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/session" {
			t.Fatalf("path = %s, want /session", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer upstream.Close()

	proxy := httptest.NewServer(NewHandler(mustParseTestURL(t, upstream.URL), ""))
	defer proxy.Close()

	resp, err := proxy.Client().Get(proxy.URL + "/session")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusOK)
	}
}

func mustParseTestURL(t *testing.T, raw string) *url.URL {
	t.Helper()
	target, err := url.Parse(raw)
	if err != nil {
		t.Fatalf("url.Parse() error = %v", err)
	}
	return target
}
