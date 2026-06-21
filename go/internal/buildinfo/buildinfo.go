// Package buildinfo exposes the Git commit and build time stamped into the
// binaries at release time via the linker (-ldflags -X). The defaults below
// stand in for an unstamped build (a plain `go build`, `go run`, or `go test`)
// so a binary always reports *something* rather than an empty string.
//
// The release path stamps these:
//
//	scripts/hosting/release.sh   (self-hosted systemd/cron deployment)
//	go/Dockerfile                (container build, via --build-arg)
//
// Session 1 of the launch-hardening audit found the deployed binaries carried
// no embedded VCS revision (`go version -m` reported `(devel)`), so a running
// process could not be mapped to one source commit without binary-hash
// archaeology. Stamping Commit/BuildTime closes that gap: the API logs them at
// startup and reports them at GET /.
package buildinfo

var (
	// Commit is the short Git commit the binary was built from. Overwritten at
	// build time via -ldflags "-X .../buildinfo.Commit=<sha>".
	Commit = "unknown"
	// BuildTime is the UTC build timestamp (RFC3339). Overwritten at build time
	// via -ldflags "-X .../buildinfo.BuildTime=<ts>".
	BuildTime = "unknown"
)

// String returns a compact "<commit> (built <buildtime>)" descriptor for logs.
func String() string {
	return Commit + " (built " + BuildTime + ")"
}
