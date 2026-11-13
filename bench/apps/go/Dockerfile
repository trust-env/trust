FROM golang:1.23-alpine AS build
WORKDIR /src
COPY main.go .
RUN go mod init bench && go get github.com/jackc/pgx/v5@v5.7.1 && go mod tidy && CGO_ENABLED=0 go build -ldflags="-s -w" -o /app .

FROM alpine:3.20
COPY --from=build /app /app
EXPOSE 8080
CMD ["/app"]
