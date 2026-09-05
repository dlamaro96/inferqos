package inferqos
import("net/http";"testing";"time")
func TestApply(t *testing.T){r,_:=http.NewRequest("POST","http://localhost",nil);if err:=Apply(r,"interactive",3*time.Second,true);err!=nil{t.Fatal(err)};if got:=r.Header.Get("X-InferQoS-Deadline-Ms");got!="3000"{t.Fatalf("got %s",got)}}

