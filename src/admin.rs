// Provides an admin web page
// Future plans: JSON output and a prometheus 

// Prometheus output:
/*
$ curl -vvvv 0:9990/admin/metrics/prometheus
*   Trying 0.0.0.0...
* Connected to 0 (127.0.0.1) port 9990 (#0)
> GET /admin/metrics/prometheus HTTP/1.1
> Host: 0:9990
> User-Agent: curl/7.50.1
> Accept: *\/*
>
< HTTP/1.1 200 OK
< Content-Type: text/plain
< Content-Length: 17753
*/


// For histograms
/*
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="count"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="sum"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="avg"} 0.0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="min"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="max"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="stddev"} 0.0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="p50"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="p90"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="p95"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="p99"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="p9990"} 0
rt:int:srv:0_0_0_0:4140:transit_latency_ms{stat="p9999"} 0
*/

// For counters
/*
finagle:clientregistry:initialresolution_ms 0
clnt:zipkin_tracer:loadbalancer:updates 1
clnt:zipkin_tracer:tries:success 0
clnt:zipkin_tracer:retries:cannot_retry 0
clnt:zipkin_tracer:retries:not_open 0
clnt:zipkin_tracer:retries:request_limit 0
jvm:postGC:Par_Eden_Space:max 2.7918336E8
jvm:classes:total_loaded 6905.0
jvm:mem:current:Compressed_Class_Space:used 6207488.0
jvm:postGC:Par_Eden_Space:used 0.0
rt:int:srv:0_0_0_0:4140:connections 0.0
jvm:mem:buffer:direct:used 525313.0
jvm:thread:daemon_count 17.0
jvm:gc:ParNew:cycles 19.0
finagle:future_pool:active_tasks 0.0
toggles:com_twitter_finagle_netty4:checksum 2.180319744E9
*/
