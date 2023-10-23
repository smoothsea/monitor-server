#######################BUILD IAMGE################
FROM rust:1.48.0 as build
RUN mkdir /app && cd /app
ADD ./ /app/monitor-server
WORKDIR /app/monitor-server
RUN rustup default nightly-2022-03-15
RUN cargo build --release -p proxy -p monitor_server

#######################RUNTIME IMAGE##############
FROM debian:buster-slim
RUN apt-get update && apt-get install -y \
            --no-install-recommends \
            openssl \
            ca-certificates \
	    libsqlite3-0
ENV TZ=Asia/Shanghai
RUN ln -snf /usr/share/zoneinfo/$TZ /etc/localtime && echo $TZ > /etc/timezone 
RUN mkdir /data
COPY --from=build app/monitor-server/target/release/monitor_server .
COPY --from=build app/monitor-server/target/release/proxy .
COPY --from=build app/monitor-server/templates ./templates/
WORKDIR /
CMD ["/monitor_server"]
