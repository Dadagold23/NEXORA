# Container image for an NXL application — an evaluated option, not a
# requirement. Containers are one packaging choice among the systemd unit
# and bare-metal runs documented in docs/16-deployment.md; NXL artifacts
# are plain JavaScript and need nothing container-specific.

FROM node:22-slim

# Build stage: compile the NXL source to JavaScript. Replace the paths with
# the project's real entry point.
#
#   COPY . .
#   RUN nxc build --release
#
# The compiler is not shipped in the final image; only the artifact is.

WORKDIR /app
COPY . .

# The runtime image contains just the built artifact and node.
# RUN nxc build --release   # uncomment once the compiler is available here

ENV NODE_ENV=production
ENV NXL_ENV=production

# Document the expected runtime configuration without baking in values.
# Supply real values at run time:  --env-file app.env
ENV PORT=8080

EXPOSE 8080

# No shell, no package manager surprises: the entrypoint is the artifact.
CMD ["node", "build/release/main.js"]
