package main

import (
	"context"
	"crypto/tls"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"log"
	"net"
	"net/http"
	"net/http/cookiejar"
	"os"
	"time"

	"github.com/paultyng/go-unifi/unifi"
)

// ZoneRecord A DNS entry.
type ZoneRecord struct {
	Name   string   `json:"name"`
	Type   string   `json:"type"`
	TTL    int      `json:"ttl"`
	Values []string `json:"values"`
}

// Zone A list of ZoneRecords
type Zone map[string]ZoneRecord

func setHTTPClient(c *unifi.Client, insecure bool) {
	httpClient := &http.Client{}
	httpClient.Transport = &http.Transport{
		Proxy: http.ProxyFromEnvironment,
		DialContext: (&net.Dialer{
			Timeout:   30 * time.Second,
			KeepAlive: 30 * time.Second,
			DualStack: true,
		}).DialContext,
		MaxIdleConns:          100,
		IdleConnTimeout:       90 * time.Second,
		TLSHandshakeTimeout:   10 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,

		TLSClientConfig: &tls.Config{
			InsecureSkipVerify: insecure,
		},
	}

	jar, _ := cookiejar.New(nil)
	httpClient.Jar = jar

	c.SetHTTPClient(httpClient)
}

func readZone() (Zone, error) {
	fileContent, err := os.Open("sully.org.json")
	if err != nil {
		return nil, err
	}

	defer fileContent.Close()

	byteResult, _ := ioutil.ReadAll(fileContent)

	var zone Zone

	err = json.Unmarshal([]byte(byteResult), &zone)

	return zone, err
}

func readUniFi() (map[string]string, error) {
	baseURL := "https://unifi.sully.org/"
	// devices := Devices{}

	devices := make(map[string]string)

	client := &unifi.Client{}
	ctx := context.Background()

	user := "dsully"
	pass := "tpbHWf79TmBfdysigBrwwM-JmeYj!XxR"

	setHTTPClient(client, false)

	err := client.SetBaseURL(baseURL)
	if err != nil {
		return devices, err
	}

	err = client.Login(ctx, user, pass)
	if err != nil {
		return devices, err
	}

	users, err := client.ListUser(ctx, "default")
	if err != nil {
		return devices, err
	}

	for _, u := range users {
		ip := ""

		if u.UseFixedIP {
			ip = u.FixedIP
		} else {
			ip = u.IP
		}

		if ip == "" || u.Name == "" {
			continue
		}

		devices[u.Name] = ip
	}

	return devices, nil
}

func main() {
	devices, err := readUniFi()
	if err != nil {
		log.Fatal(err)
	}

	zone, err := readZone()
	if err != nil {
		log.Fatal(err)
	}

	for i, zoneEntry := range zone {

		if zoneEntry.Type != "A" {
			continue
		}

		for name, ip := range devices {
			if zoneEntry.Name == name {
				for _, v := range zoneEntry.Values {
					if ip != v {
						// fmt.Printf("Mismatched IP: %s != %s for %s\n", v, ip, name)
						zoneEntry.Values = []string{ip}
					}
				}
			}
		}

		zone[i] = zoneEntry
	}

	jsonByte, _ := json.Marshal(zone)

	fmt.Println(string(jsonByte))
}
