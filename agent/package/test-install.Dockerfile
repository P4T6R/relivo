ARG BASE=debian:12-slim
FROM ${BASE}
COPY relivo-agent_1.5.3+relivo.20261111.1_amd64.deb /tmp/relivo-agent.deb
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y /tmp/relivo-agent.deb \
    && rm -rf /var/lib/apt/lists/* /tmp/relivo-agent.deb \
    && useradd --create-home tester
USER tester
ENV PYTHONDONTWRITEBYTECODE=1
CMD ["relivo-agent", "--version"]
