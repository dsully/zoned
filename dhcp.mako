#!/bin/vbash

source /opt/vyatta/etc/functions/script-template
configure

delete service dhcp-server shared-network-name LAN subnet ${subnet}

set service dhcp-server shared-network-name LAN subnet ${subnet}
set service dhcp-server shared-network-name LAN subnet ${subnet} default-router ${router}
set service dhcp-server shared-network-name LAN subnet ${subnet} dns-server ${dns[0]}
set service dhcp-server shared-network-name LAN subnet ${subnet} dns-server ${dns[1]}
set service dhcp-server shared-network-name LAN subnet ${subnet} domain-name ${domain}
set service dhcp-server shared-network-name LAN subnet ${subnet} lease ${lease}
set service dhcp-server shared-network-name LAN subnet ${subnet} start ${range[0]}
set service dhcp-server shared-network-name LAN subnet ${subnet} start ${range[0]} stop ${range[1]}
set service dhcp-server shared-network-name LAN subnet ${subnet} unifi-controller ${unifi}

% for record in records:
set service dhcp-server shared-network-name LAN subnet ${subnet} static-mapping ${record['name']} ip-address ${record['ip']}
set service dhcp-server shared-network-name LAN subnet ${subnet} static-mapping ${record['name']} mac-address ${record['mac']}
% endfor

commit
save
exit
