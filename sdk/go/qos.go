// Package inferqos adds optional QoS metadata to ordinary HTTP requests.
package inferqos
import("fmt";"net/http";"strconv";"time")
func Apply(r *http.Request,class string,deadline time.Duration,queueable bool)error{switch class{case"realtime","interactive","standard","workflow","batch":default:return fmt.Errorf("unknown InferQoS service class %q",class)};if deadline<=0{return fmt.Errorf("deadline must be positive")};r.Header.Set("X-InferQoS-Class",class);r.Header.Set("X-InferQoS-Deadline-Ms",strconv.FormatInt(deadline.Milliseconds(),10));r.Header.Set("X-InferQoS-Queueable",strconv.FormatBool(queueable));return nil}

