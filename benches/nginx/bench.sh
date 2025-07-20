docker run -d --name nginx-cache -p 8080:80 -v $(pwd)/nginx.conf:/etc/nginx/nginx.conf:ro -v $(pwd)/cache:/var/cache/nginx nginx
k6 run script.js
docker stop nginx-cache
docker rm nginx-cache

docker stats nginx-cache


docker run -d --name cacheus -p 8081:80 -v $(pwd)/cacheus.yml:/etc/cacheus.yml ghcr.io/ogxd/cacheus:0.1.0
