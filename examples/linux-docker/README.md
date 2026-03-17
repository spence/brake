# Linux testing

The thread-lanes library uses cgroup v2 on Linux. If you're developing on macOS
and want to verify the Linux backend, build and run the examples in Docker:

    docker build -t thread-lanes -f examples/linux-docker/Dockerfile ../..
    docker run --rm --privileged thread-lanes

The `--privileged` flag is required for cgroup v2 access. The default entrypoint
runs `prove_all`, which exercises saturation, demotion/promotion, and per-thread
CPU accounting.

To run a specific example:

    docker run --rm --privileged thread-lanes /app/show_threads
